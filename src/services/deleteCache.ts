import { deleteKeys } from "@/libs/redis";
import { deleteFolder } from "@/libs/s3";

export const deleteCache = async () => {
	console.log("Deleting cache");

	try {
		//Delete s3 folder
		await deleteFolder("cached/");
	} catch (error) {
		console.error("Error deleting cache", error);
		throw new Error("Error deleting cache");
	}

	try {
		//Delete keys in redis
		await deleteKeys("cache:*");
	} catch (error) {
		console.error("Error deleting redis cache", error);
		throw new Error("Error deleting redis cache");
	}

	console.log("Cache deleted");
};
