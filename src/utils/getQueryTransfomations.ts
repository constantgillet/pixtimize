/**
 * Get the transformations from the query params
 * @param query
 * @returns
 */
export const getQueryTransformations = (query: {
	tr?: string;
}) => {
	if (!query.tr) {
		return [];
	}

	const transformations = query.tr.split(",");

	return transformations;
};
