import { GetObjectCommand, S3Client } from "@aws-sdk/client-s3";
import { environment } from "./environment";

const s3 = new S3Client({
	forcePathStyle: false,
	endpoint: environment().S3_ENDPOINT,
	region: environment().S3_REGION,
	credentials: {
		accessKeyId: environment().S3_ACCESS_KEY,
		secretAccessKey: environment().S3_SECRET_KEY,
	},
});

/**
 * Get a file from the S3 server based on the key
 * @param key key of the file, the path of the file in the S3 server
 * @returns
 */
export const getFile = async (key: string) => {
	const params = {
		Bucket: environment().S3_BUCKET,
		Key: key,
	};

	const data = await s3.send(new GetObjectCommand(params));

	return data;
};
