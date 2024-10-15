import { error } from "elysia";
import { z } from "zod";
import { getPathTransformations } from "@/utils/getPathTransformations";
import { getQueryTransformations } from "@/utils/getQueryTransformations";
import { mapTransformations } from "@/utils/mapTransformations";
import { environment } from "@/libs/environment";
import { getImagePath } from "@/utils/getImagePath";
import { getCacheData, setCacheData } from "@/libs/redis";
import { getFile, uploadToS3 } from "@/libs/s3";
import sharp, { type Sharp } from "sharp";
import type { GetObjectCommandOutput } from "@aws-sdk/client-s3";

const cacheTime = environment().CACHED_TIME; // Default is 604800

//transformations is tr:w-300,h-300 in the path or tr=w-518%2Ch-450 in the query params
const getTransformations = (
	path: string,
	query: {
		tr?: string;
	},
) => {
	const pathTransformations = getPathTransformations(path);
	const queryTransformations = getQueryTransformations(query);

	// const queryTransformations = getQueryTransformations(query);
	const transformations = mapTransformations([
		...pathTransformations,
		...queryTransformations,
	]);
	return transformations;
};

const formatsSchema = z.enum(["webp", "jpg", "jpeg", "png"]);

type Formats = z.infer<typeof formatsSchema>;

const transformationsSchema = z.object({
	w: z
		.string()
		.optional()
		.transform((value) => {
			return value === undefined ? undefined : Number.parseFloat(value);
		}), //width of the image in pixels or percentage if val < 1 and > 0
	h: z
		.string()
		.optional()
		.transform((value) => {
			return value === undefined ? undefined : Number.parseFloat(value);
		}),
	q: z
		.string()
		.default(environment().DEFAULT_QUALITY)
		.transform((value) => Number.parseFloat(value)), //quality of the image
	f: formatsSchema.optional().default(environment().DEFAULT_FORMAT), //format of the image
});

export const renderImage = async ({
	path,
	query,
	request,
}: {
	path: string;
	query: {
		tr?: string;
	};
	request: Request;
}) => {
	//Split the path to get the image name
	const pathSplited: Array<string> = path.split("/");

	const isPathTransform = pathSplited[1].includes("tr");

	const imagePath = getImagePath(path, isPathTransform);
	const transformations = getTransformations(path, query);

	//Get valid transformations
	const transformationsValidated = transformationsSchema.parse(transformations);

	//Create an image hash based on the image path and transformations
	const imageHashInput = `${imagePath}-${JSON.stringify(
		transformationsValidated,
	)}`;

	const hasher = new Bun.CryptoHasher("sha256");
	hasher.update(imageHashInput);

	const imageHash = hasher.digest().toString("hex");
	const cachePathKey = `cached/${imageHash}`;
	const cacheKey = `cache:${cachePathKey}`;

	//Check if the image exists in redis
	const cacheData = await getCacheData(cacheKey);

	//If the image exists in the cache, redirect to the image path
	if (cacheData && environment().MODE === "redirect") {
		return new Response(null, {
			status: 301,
			headers: {
				Location: `${environment().BUCKET_URL}/${cachePathKey}`,
				"Cache-Control": "public, max-age=604800, immutable",
			},
		});
	}

	const headers = {
		"Content-Type": "image/webp",
		"Cache-Control": `public, max-age=${cacheTime}, must-revalidate`, //default cache time is one week 604800 seconds
		Expires: new Date(Date.now() + cacheTime).toUTCString(),
		Accept: "*/*",
	};

	try {
		if (cacheData && environment().MODE === "remote") {
			const file = await getFile(cachePathKey);
			const imageBody = await file.Body?.transformToByteArray();

			// Handle both GET and HEAD requests
			if (request.method === "HEAD") {
				return new Response(null, {
					status: 200,
					headers: {
						...headers,
						"Content-Length": imageBody ? imageBody.length.toString() : "0",
					},
				});
			}
			return new Response(imageBody, { headers });
		}
	} catch (e) {
		if (e instanceof Error && e.name === "NoSuchKey") {
			return error(404, "Image not found");
		}
		console.error("Error getting image", e);
		return error(500, "Internal server error");
	}

	let imageRes: GetObjectCommandOutput | undefined;

	try {
		imageRes = await getFile(imagePath);
	} catch (e) {
		if (e instanceof Error && e.name === "NoSuchKey") {
			return error(404, "Image not found");
		}

		console.error("Error getting image", e);
		return error(500, "Internal server error");
	}

	//If the image doesn't exist, return a 404
	if (!imageRes) {
		return error(404, "Image not found");
	}

	const buffer = await imageRes.Body?.transformToByteArray();

	//Apply the transformations to the image
	const sharpedImage = sharp(buffer);

	const image = await withFormat(
		sharpedImage.resize({
			width: transformationsValidated.w,
			height: transformationsValidated.h,
		}),
		transformationsValidated.f,
		transformationsValidated.q,
	).toBuffer();

	void saveImageInCache(cachePathKey, cacheKey, image);

	const contentType = `image/${transformationsValidated.f}`;

	//Handle both GET and HEAD requests for newly processed images
	if (request.method === "HEAD") {
		return new Response(null, {
			status: 200,
			headers: {
				...headers,
				"Content-Type": contentType,
				"Content-Length": image.length.toString(),
			},
		});
	}
	return new Response(image, {
		headers: {
			...headers,
			"Content-Type": contentType,
		},
	});
};

const withFormat = (image: Sharp, format: Formats, quality: number) => {
	switch (format) {
		case "webp":
			return image.webp({ quality });
		case "jpg":
		case "jpeg":
			return image.jpeg({ quality });
		case "png":
			return image.png({ quality });
	}
};

const saveImageInCache = async (
	key: string,
	cacheKey: string,
	data: Buffer,
) => {
	//Save the image in the cache
	await uploadToS3(data, key);
	await setCacheData(cacheKey, key);
};
