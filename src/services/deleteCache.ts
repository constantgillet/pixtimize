import { deleteKeys } from "@/libs/redis";
import { deleteFolder } from "@/libs/s3";

export const deleteCache = async () => {
	try {
		//Delete s3 folder
		deleteFolder("cached/");
	} catch (error) {
		console.error("Error deleting cache", error);
		throw new Error("Error deleting cache");
	}

	try {
		//Delete keys in redis
		deleteKeys("cache:*");
	} catch (error) {
		console.error("Error deleting redis cache", error);
		throw new Error("Error deleting redis cache");
	}
};
