# Pixtimize ⚡🖼️

Pixtimize is an opensource image transform api compatible with imagekit API. Pixtimize is compatbile with any S3 bucket service.

We use bun for having a blasting fast API

# Tools used 

- Bun and Elysiajs as web framework and runtime
- Redis to store the image cached key
- 

# Transforms compatibility

| Property name | compatible  | comment  |
|---|---|---|
| w | ✅ | fix pixel values and percentage are supported but not Sec-CH-Width |   
|  h | ✅ | fix pixel values and percentage are supported but not Sec-CH-Width |   
| q | ✅ | quality of the image, default value is DEFAULT_QUALITY .env
| f | ✅ | quality of the image, default value is DEFAULT_FORMAT .env, values are jpeg, jpg, png, webp

# Usage
You can transform the image by calling your url like this, don't forget encoding the & param

```
https://yourdomain.com/image-example.png?tr=w-606%2Ch-450%2Cf-jpeg
```
or

```
https://yourdomain.com/tr:w-606%2Ch-450%2Cf-jpeg/image-example.png
```

# Configuration

Some environment variables are required:

- `S3_ENDPOINT`: URL of the s3 bucket
- `S3_BUCKET`: name of the s3 bucket
- `S3_KEY`: accessKeyId of the s3 bucket
- `S3_SECRET`: Secret key of the s3 bucket
- `REDIS_URL`: Redis URL

Make sure to set these environment variables before running the application.

Optionnal environment variables

- `DEFAULT_QUALITY`: The quality which will be rendered by default to optimize the image
- `DEFAULT_FORMAT`: The format which will be rendered by default
- `MODE`: "redirect" default value to redirect to the cached image or "remote" to download and resend the image cached

# Commands

To start the project:

To install the packages

```bash
bun i
```

To start the project

```bash
bun start
```

To start the project as dev mode

```bash
bun dev
```
