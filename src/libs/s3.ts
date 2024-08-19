import {
	GetObjectCommand,
	PutObjectCommand,
	type PutObjectCommandInput,
	S3Client,
	ListObjectsV2Command,
	DeleteObjectsCommand,
} from "@aws-sdk/client-s3";
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

/**
 *
 * @param fileContent
 * @param key ex "ogimages/generated/test.png"
 * @returns
 */
export const uploadToS3 = async (fileContent: Buffer, key: string) => {
	const params: PutObjectCommandInput = {
		Bucket: environment().S3_BUCKET,
		Key: key,
		Body: fileContent,
		ACL: "public-read",
		ContentType: "image/png",
	};

	const command = new PutObjectCommand(params);

	try {
		const data = await s3.send(command);
		return data;
	} catch (error) {
		console.error(error);
		throw new Error("Error uploading file to S3");
	}
};

/**
 * Delete a folder and all its content from S3
 * @param location ex "cached/"
 */
export async function deleteFolder(location: string) {
	let count = 0; // number of files deleted
	async function recursiveDelete(token: string | undefined = undefined) {
		// get the files
		const listCommand = new ListObjectsV2Command({
			Bucket: environment().S3_BUCKET,
			Prefix: location,
			ContinuationToken: token,
		});
		const list = await s3.send(listCommand);
		if (list.KeyCount) {
			// if items to delete
			// delete the files
			const deleteCommand = new DeleteObjectsCommand({
				Bucket: environment().S3_BUCKET,
				Delete: {
					Objects: list.Contents?.map((item) => ({ Key: item.Key })),
					Quiet: false,
				},
			});
			const deleted = await s3.send(deleteCommand);

			if (deleted.Deleted) count += deleted.Deleted.length;

			// log any errors deleting files
			if (deleted.Errors) {
				deleted.Errors.map((error) =>
					console.log(`${error.Key} could not be deleted - ${error.Code}`),
				);
			}
		}

		// repeat if more files to delete
		if (list.NextContinuationToken) {
			recursiveDelete(list.NextContinuationToken);
		}
		// return total deleted count when finished
		return {
			deletedCount: count,
		};
	}
	// start the recursive function
	return recursiveDelete();
}
