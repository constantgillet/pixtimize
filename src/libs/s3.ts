import { S3Client } from "@aws-sdk/client-s3";
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
