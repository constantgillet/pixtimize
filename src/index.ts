import { Elysia } from "elysia";
import { environment } from "./libs/environment";
import cron from "@elysiajs/cron";
import { deleteCache } from "@/services/deleteCache";
import { renderImage } from "@/services/renderImage";

const app = new Elysia()
  .use(
    cron({
      name: "delete-cache",
      pattern: environment().CACHE_DELETE_CRON, //Every Monday at 1:00
      run() {
        deleteCache();
      },
    })
  )
  .get("/", () => {
    //Return 200 OK
    return new Response("OK", { status: 200 });
  })
  .get("/*", renderImage)
  .listen(environment().PORT);

console.log(
  `🦊 Elysia is running at ${app.server?.hostname}:${app.server?.port}`
);

console.log(environment());
