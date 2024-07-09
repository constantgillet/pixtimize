import { Elysia } from "elysia";

const getImagePath = (path: string, isPathTransform: boolean): string => {
	const pathSplited: Array<string> = path.split("/");
	let imagePath = "";

	for (let i = isPathTransform ? 2 : 1; i < pathSplited.length; i++) {
		imagePath += `/${pathSplited[i]}`;
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

const renderImage = async ({ path, query }) => {
	//Split the path to get the image name
	const pathSplited: Array<string> = path.split("/");

	const isPathTransform = pathSplited[1].includes("tr");

	const imagePath = getImagePath(path, isPathTransform);
	const transformations = getTransformations(path, query);

	console.log(transformations);

	//Get valid transformations

	//Create an image hash

	//Check if the image exists in the cache

	//If the image exists in the cache, redirect to the image path

	return imagePath;
};

const app = new Elysia().get("/*", renderImage).listen(3000);

console.log(
	`🦊 Elysia is running at ${app.server?.hostname}:${app.server?.port}`,
);
