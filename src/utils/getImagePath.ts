/**
 * Get the image path from the URL
 * @param path
 * @param isPathTransform
 * @returns
 */
export const getImagePath = (
	path: string,
	isPathTransform: boolean,
): string => {
	const pathSplited: Array<string> = path.split("/");
	let imagePath = "";
	const initialIndex = isPathTransform ? 2 : 1;

	for (let i = initialIndex; i < pathSplited.length; i++) {
		imagePath += `${i === initialIndex ? "" : "/"}${pathSplited[i]}`;
	}

	return imagePath;
};
