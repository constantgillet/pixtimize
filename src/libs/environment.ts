import * as z from "zod";

const environmentSchema = z.object({
	NODE_ENV: z
		.enum(["development", "production", "test"])
		.default("development"),
	S3_ENDPOINT: z.string().default("https://ams3.digitaloceanspaces.com"),
	S3_REGION: z.string().default("ams3"),
	S3_ACCESS_KEY: z.string(),
	S3_SECRET_KEY: z.string(),
	S3_BUCKET: z.string(),
});

const environment = () => environmentSchema.parse(process.env);

export { environment };
