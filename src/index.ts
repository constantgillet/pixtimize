import { Elysia, error } from "elysia";
import { z } from "zod";
import { getFile, uploadToS3 } from "./libs/s3";
import { environment } from "./libs/environment";
import sharp from "sharp";
import { getCacheData, setCacheData } from "./libs/redis";
import cron from "@elysiajs/cron";

const getImagePath = (path: string, isPathTransform: boolean): string => {
	const pathSplited: Array<string> = path.split("/");
	let imagePath = "";
	const initialIndex = isPathTransform ? 2 : 1;

	for (let i = initialIndex; i < pathSplited.length; i++) {
		imagePath += `${i === initialIndex ? "" : "/"}${pathSplited[i]}`;
	}

	return imagePath;
};

const mapTransformations = (transformations: Array<string>) => {
	const transformationsList: { [key: string]: string } = {};

	for (let i = 0; i < transformations.length; i++) {
		const transformation = transformations[i].split("-");
		const key = transformation[0];
		const value = transformation[1];

		//TODO copatibility with ar-
		transformationsList[key] = value;
	}

	return transformationsList;
};

//transformations is tr:w-300,h-300 in the path or tr=w-518%2Ch-450 in the query params
const getTransformations = (path: string, query) => {
	const pathTransformations = getPathTransformations(path);
	const queryTransformations = getQueryTransformations(query);

	// const queryTransformations = getQueryTransformations(query);
	const transformations = mapTransformations([
		...pathTransformations,
		...queryTransformations,
	]);
	return transformations;
};

const getPathTransformations = (path: string) => {
	const pathSplited: Array<string> = path.split("/");
	const firstPath = pathSplited[1];
	if (!firstPath.includes("tr")) {
		return [];
	}

	const transformations = pathSplited[1].split(",");
	transformations[0] = transformations[0].replace("tr:", "");

	return transformations;
};

const getQueryTransformations = (query) => {
	if (!query.tr) {
		return [];
	}

	const transformations = query.tr.split(",");

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

const renderImage = async ({ path, query }) => {
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

const app = new Elysia()
	.use(
		cron({
			name: "heartbeat",
			pattern: "0 1 * * 1", //Every Monday at 1:00
			run() {
				console.log("Heartbeat");
			},
		}),
	)
	.get("/*", renderImage)
	.listen(environment().PORT);

console.log(
	`🦊 Elysia is running at ${app.server?.hostname}:${app.server?.port}`,
);
