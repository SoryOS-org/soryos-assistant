import { Hono } from "hono";

import { getWorkerAuthToken } from "./config";
import type { WorkerBindings } from "./config";
import { registerChatRoutes } from "./routes/chat";
import { registerMachineStateRoutes } from "./routes/machine_state";
import { registerPlannerRoutes } from "./routes/planner";

const app = new Hono<{ Bindings: WorkerBindings }>();

app.get("/health", (c) => c.text("ok"));

app.use("*", async (c, next) => {
  if (c.req.path === "/health") {
    return next();
  }

  let expectedToken: string;
  try {
    expectedToken = getWorkerAuthToken(c.env);
  } catch (error) {
    const message =
      error instanceof Error ? error.message : "Worker auth token not configured";
    return c.text(message, 500);
  }

  const authHeader = c.req.header("Authorization") || "";
  if (!authHeader.startsWith("Bearer ")) {
    return c.text("Unauthorized", 401);
  }

  const providedToken = authHeader.slice("Bearer ".length).trim();
  if (!providedToken || providedToken !== expectedToken) {
    return c.text("Unauthorized", 403);
  }

  return next();
});

registerChatRoutes(app);
registerPlannerRoutes(app);
registerMachineStateRoutes(app);

export default app;
export { MachineStateDO } from "./machine-state-do";
