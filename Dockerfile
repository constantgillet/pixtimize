FROM oven/bun

WORKDIR /app

COPY package.json .
COPY bun.lockb .

RUN bun install --production

COPY src src
COPY tsconfig.json .
# COPY public public

ENV NODE_ENV production
ENV PORT 3000
ENV S3_ENDPOINT
ENV S3_BUCKET
ENV S3_ACCESS_KEY
ENV S3_SECRET_KEY
ENV S3_REGION
ENV DEFAULT_QUALITY
ENV REDIS_URL
ENV BUCKET_URL
ENV CACHE_DELETE_CRON

CMD ["bun", "src/index.ts"]

EXPOSE 3000