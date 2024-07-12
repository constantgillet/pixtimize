import { error } from "elysia";
import { z } from "zod";
import { getPathTransformations } from "@/utils/getPathTransformations";
import { getQueryTransformations } from "@/utils/getQueryTransformations";
import { mapTransformations } from "@/utils/mapTransformations";
import { environment } from "@/libs/environment";
import { getImagePath } from "@/utils/getImagePath";
import { getCacheData, setCacheData } from "@/libs/redis";
import { getFile, uploadToS3 } from "@/libs/s3";
import sharp from "sharp";

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
});

export const renderImage = async ({
	path,
	query,
}: {
	path: string;
	query: {
		tr?: string;
	};
}) => {
	//Split the path to get the image name
	const pathSplited: Array<string> = path.split("/");

	const isPathTransform = pathSplited[1].includes("tr");

	const imagePath = getImagePath(path, isPathTransform);
	const transformations = getTransformations(path, query);

	//Get valid transformations
	const transformationsValidated = transformationsSchema.parse(transformations);

	//Create an image hash based on the image path and transformations
	const imageHashInput = `${imagePath}-${JSON.stringify(transformationsValidated)}`;

	const hasher = new Bun.CryptoHasher("sha256");
	hasher.update(imageHashInput);

	const imageHash = hasher.digest().toString("hex");
	const cachePathKey = `cached/${imageHash}`;

	//Check if the image exists in redis
	const cacheData = await getCacheData(cachePathKey);

	//If the image exists in the cache, redirect to the image path
	if (cacheData) {
		return new Response(null, {
			status: 301,
			headers: {
				Location: `${environment().BUCKET_URL}/${cachePathKey}`,
			},
		});
	}

	console.log(imagePath);

	//Get the image from s3 server
	const imageRes = await getFile(imagePath);

	//If the image doesn't exist, return a 404
	if (!imageRes) {
		return error(404, "Image not found");
	}

	const buffer = await imageRes.Body?.transformToByteArray();

	//Apply the transformations to the image
	const sharpedImage = sharp(buffer);
	const image = await sharpedImage
		.resize({
			width: transformationsValidated.w,
			height: transformationsValidated.h,
		})
		.webp({
			quality: transformationsValidated.q,
		})
		.toBuffer();

	void saveImageInCache(cachePathKey, image);

	//return the image
	return new Response(image, {
		headers: {
			"Content-Type": "image/webp",
		},
	});
};

const saveImageInCache = async (key: string, data: Buffer) => {
	//Save the image in the cache
	await uploadToS3(data, key);
	await setCacheData(key, JSON.stringify(true));
};
