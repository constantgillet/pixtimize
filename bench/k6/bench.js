// Load generator for one (target, image, scenario) combination.
//
// Scenarios:
//   cold - every request asks for a never-seen transform (unique width/quality),
//          so each service must fetch the source, decode, resize and encode.
//   warm - every request asks for the same transform. Pixtimize/ImageKit serve
//          it from their cache; imgproxy/imaginary have no cache and re-render.

import http from "k6/http";
import { check } from "k6";
import exec from "k6/execution";
import { Trend } from "k6/metrics";

const TARGET = __ENV.TARGET;
const BASE_URL = __ENV.BASE_URL.replace(/\/$/, "");
const IMAGE = __ENV.IMAGE;
const SCENARIO = __ENV.SCENARIO || "cold";
const SEED = Number(__ENV.SEED || 0);
const SOURCE_BASE = __ENV.SOURCE_BASE || "http://minio:9000/bench";

const respBytes = new Trend("resp_bytes");

export const options = {
  scenarios: {
    bench: {
      executor: "constant-vus",
      vus: Number(__ENV.VUS || 8),
      duration: __ENV.DURATION || "30s",
    },
  },
  summaryTrendStats: ["avg", "min", "med", "max", "p(90)", "p(95)", "p(99)"],
  discardResponseBodies: false,
};

function url(width, quality) {
  switch (TARGET) {
    case "pixtimize":
    case "pixtimize-remote":
    case "imagekit":
      return `${BASE_URL}/tr:w-${width},q-${quality},f-webp/${IMAGE}`;
    case "imgproxy":
      return `${BASE_URL}/insecure/rs:fit:${width}:0/q:${quality}/plain/s3://bench/${IMAGE}@webp`;
    case "imaginary":
      return `${BASE_URL}/resize?width=${width}&quality=${quality}&type=webp&url=${encodeURIComponent(`${SOURCE_BASE}/${IMAGE}`)}`;
    default:
      throw new Error(`unknown TARGET ${TARGET}`);
  }
}

function transformFor(iteration) {
  if (SCENARIO === "warm") return [800, 80];
  // 1800 widths x 40 qualities = 72k unique transforms per image.
  const i = iteration + SEED;
  return [200 + (i % 1800), 50 + (Math.floor(i / 1800) % 40)];
}

export function setup() {
  // Prime the warm URL (and fail fast if the target is misconfigured).
  const res = http.get(url(800, 80));
  if (res.status !== 200) {
    throw new Error(`${TARGET} returned ${res.status} for ${url(800, 80)}: ${String(res.body).slice(0, 200)}`);
  }
}

export default function () {
  const [width, quality] = transformFor(exec.scenario.iterationInTest);
  const res = http.get(url(width, quality), { tags: { name: TARGET } });
  check(res, {
    "status 200": (r) => r.status === 200,
    "is webp": (r) => String(r.headers["Content-Type"] || "").includes("image/webp"),
  });
  if (res.status === 200) respBytes.add(res.body.length);
}

export function handleSummary(data) {
  const d = data.metrics.http_req_duration.values;
  const result = {
    target: TARGET,
    scenario: SCENARIO,
    image: IMAGE,
    vus: Number(__ENV.VUS || 8),
    duration: __ENV.DURATION || "30s",
    requests: data.metrics.http_reqs.values.count,
    rps: data.metrics.http_reqs.values.rate,
    error_rate: data.metrics.http_req_failed.values.rate,
    avg_ms: d.avg,
    p50_ms: d.med,
    p90_ms: d["p(90)"],
    p95_ms: d["p(95)"],
    p99_ms: d["p(99)"],
    max_ms: d.max,
    avg_bytes: data.metrics.resp_bytes ? data.metrics.resp_bytes.values.avg : 0,
  };
  const line =
    `${TARGET.padEnd(16)} ${SCENARIO.padEnd(5)} ${IMAGE.padEnd(16)} ` +
    `${result.rps.toFixed(1).padStart(8)} req/s  p50 ${result.p50_ms.toFixed(1)}ms  ` +
    `p95 ${result.p95_ms.toFixed(1)}ms  p99 ${result.p99_ms.toFixed(1)}ms  ` +
    `errors ${(result.error_rate * 100).toFixed(2)}%\n`;
  return {
    stdout: line,
    [`/results/${TARGET}-${SCENARIO}-${IMAGE.replace(/[^\w.-]/g, "_")}.json`]: JSON.stringify(result, null, 2),
  };
}
