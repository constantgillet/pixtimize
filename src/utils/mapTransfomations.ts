/**
 * Map transformations from array to object
 * @param transformations
 * @returns
 */
export const mapTransformations = (transformations: Array<string>) => {
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
