import redis, { type RedisClientType } from "redis";
let redisClient: RedisClientType | null;

(async () => {
	redisClient = redis.createClient({ url: process.env.REDIS_URL });

	redisClient.on("error", (error) => console.error(`Error : ${error}`));

	await redisClient.connect();
})();

export const setCacheData = async (key: string, data: string) => {
	try {
		const resData = await redisClient?.set(key, data);
		return resData;
	} catch (error) {
		console.error("Error caching data", error);
		throw new Error("Error caching data");
	}
};

export const getCacheData = async (key: string) => {
	try {
		const resData = await redisClient?.get(key);
		return resData;
	} catch (error) {
		console.error("Error fetching cached data", error);
	}
};

export const deleteKeys = async (keyPatern: string) => {
	if (!redisClient) {
		throw new Error("Redis client not initialized");
	}

	let cursor = 0;
	do {
		const res = await redisClient.scan(cursor, {
			MATCH: keyPatern,
			COUNT: 1000,
		});
		cursor = res.cursor;
		const keys = res.keys;

		if (keys.length > 0) {
			await redisClient.del(keys);
		}
	} while (cursor !== 0);

	return true;
};
