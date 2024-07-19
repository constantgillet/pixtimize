ARG BUN_VERSION=1.1.2

FROM oven/bun:${BUN_VERSION} AS builder

WORKDIR /app

COPY . .

RUN bun i

ENV PORT 3000
EXPOSE 3000

USER bun

CMD ["bun", "run", "start"]