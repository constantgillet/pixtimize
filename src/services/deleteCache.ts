import { deleteKeys, redisClient } from "@/libs/redis";
import { deleteFolder, deleteMultipleFiles } from "@/libs/s3";

export const deleteCache = async () => {
	if (!redisClient) {
		throw new Error("Redis client not initialized");
	}

	console.log("Deleting cache");

	let cursor = 0;
	let filesDeleted = 0;
	do {
		const res = await redisClient.scan(cursor, {
			MATCH: "cache:*",
			COUNT: 1000,
		});
		cursor = res.cursor;
		const keys = res.keys;

		if (keys.length > 0) {
			const s3Keys = keys.map((key) => key.replace("cache:", ""));
			await redisClient.del(keys);
			await deleteMultipleFiles(s3Keys);
			filesDeleted += keys.length;
		}
	} while (cursor !== 0);

	console.log(`Deleted ${filesDeleted} files from cache`);

	return true;
};
