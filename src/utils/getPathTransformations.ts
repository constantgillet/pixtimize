/**
 * Get the transformations from the path
 * @param path
 * @returns
 */
export const getPathTransformations = (path: string) => {
	const pathSplited: Array<string> = path.split("/");
	const firstPath = pathSplited[1];
	if (!firstPath.includes("tr")) {
		return [];
	}

	const transformations = pathSplited[1].split(",");
	transformations[0] = transformations[0].replace("tr:", "");

	return transformations;
};
