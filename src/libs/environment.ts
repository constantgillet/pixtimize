import * as z from "zod";

const environmentSchema = z.object({
	NODE_ENV: z
		.enum(["development", "production", "test"])
		.default("development"),
	PORT: z
		.string()
		.default("3000")
		.transform((value) => Number.parseInt(value)),
	S3_ENDPOINT: z.string().default("https://ams3.digitaloceanspaces.com"),
	S3_REGION: z.string().default("ams3"),
	S3_ACCESS_KEY: z.string(),
	S3_SECRET_KEY: z.string(),
	S3_BUCKET: z.string(),
	DEFAULT_QUALITY: z.string().default("80"),
	DEFAULT_FORMAT: z.enum(["webp", "jpg", "jpeg", "png"]).default("webp"),
	REDIS_URL: z.string(),
	BUCKET_URL: z
		.string()
		.default("https://test-image.ams3.cdn.digitaloceanspaces.com"),
	CACHE_DELETE_CRON: z.string().default("0 1 * * 1"), //every monday at 1am
	MODE: z.enum(["redirect", "remote"]).default("redirect"),
	CACHED_TIME: z
		.string()
		.default("604800")
		.transform((value) => Number.parseInt(value)), //Convert to number
});

const environment = () => environmentSchema.parse(process.env);

export { environment };
