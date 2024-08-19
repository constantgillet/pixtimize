import { deleteKeys } from "@/libs/redis";
import { deleteFolder } from "@/libs/s3";

export const deleteCache = async () => {
	console.log("Deleting cache");

	try {
		console.log("Deleting s3 folder");
		//Delete s3 folder
		await deleteFolder("cached/");
		console.log("S3 folder deleted");
	} catch (error) {
		console.error("Error deleting cache", error);
		throw new Error("Error deleting cache");
	}

	try {
		console.log("Deleting redis cache");
		//Delete keys in redis
		await deleteKeys("cache:*");
		console.log("Redis cache deleted");
	} catch (error) {
		console.error("Error deleting redis cache", error);
		throw new Error("Error deleting redis cache");
	}

	console.log("Cache deleted");
};
