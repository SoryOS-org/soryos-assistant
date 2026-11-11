import type { Context } from "hono";
import { MachineConfigSchema, MachineSyncRequestSchema } from "@sprite/shared";

import type { WorkerBindings } from "../config";
import { getMachineStateDO } from "../machine_state";

export function registerMachineStateRoutes(app: { post: Function; get: Function; delete: Function }): void {
  app.post("/sync_machine_state", async (c: Context<{ Bindings: WorkerBindings }>) => {
    try {
      const body = await c.req.json();
      const parsed = MachineSyncRequestSchema.safeParse(body);
      if (!parsed.success) {
        c.status(400);
        return c.json({ ok: false, error: `Invalid request: ${parsed.error.message}` });
      }

      const update = parsed.data;
      const stub = getMachineStateDO(update.machine_id, c.env);
      const response = await stub.fetch(new Request("http://dummy/sync"), {
        method: "POST",
        body: JSON.stringify(update),
      });

      if (!response.ok) {
        const error = await response.json().catch(() => ({ error: "Unknown error" }));
        return new Response(JSON.stringify(error), {
          status: response.status,
          headers: { "Content-Type": "application/json" },
        });
      }

      return c.json(await response.json());
    } catch (error) {
      console.error("[worker] sync_machine_state error:", error);
      c.status(500);
      return c.json({ ok: false, error: String(error) });
    }
  });

  app.get("/memory", async (c: Context<{ Bindings: WorkerBindings }>) => {
    try {
      const machineId = c.req.query("machine_id");
      if (!machineId) {
        c.status(400);
        return c.json({ ok: false, error: "missing machine_id query parameter" });
      }

      const stub = getMachineStateDO(machineId, c.env);
      const response = await stub.fetch(
        new Request(`http://dummy/memory?machine_id=${encodeURIComponent(machineId)}`),
        {
          method: "GET",
        },
      );

      if (!response.ok) {
        const error = await response.json().catch(() => ({ error: "Unknown error" }));
        return new Response(JSON.stringify(error), {
          status: response.status,
          headers: { "Content-Type": "application/json" },
        });
      }

      return c.json(await response.json());
    } catch (error) {
      console.error("[worker] memory get error:", error);
      c.status(500);
      return c.json({ ok: false, error: String(error) });
    }
  });

  app.delete("/memory", async (c: Context<{ Bindings: WorkerBindings }>) => {
    try {
      const machineId = c.req.query("machine_id");
      if (!machineId) {
        c.status(400);
        return c.json({ ok: false, error: "missing machine_id query parameter" });
      }

      const stub = getMachineStateDO(machineId, c.env);
      const response = await stub.fetch(
        new Request(`http://dummy/memory?machine_id=${encodeURIComponent(machineId)}`),
        { method: "DELETE" },
      );

      if (!response.ok) {
        const error = await response.json().catch(() => ({ error: "Unknown error" }));
        return new Response(JSON.stringify(error), {
          status: response.status,
          headers: { "Content-Type": "application/json" },
        });
      }

      return c.json(await response.json());
    } catch (error) {
      console.error("[worker] memory delete error:", error);
      c.status(500);
      return c.json({ ok: false, error: String(error) });
    }
  });

  app.get("/allowlist", async (c: Context<{ Bindings: WorkerBindings }>) => {
    try {
      const machineId = c.req.query("machine_id");
      if (!machineId) {
        c.status(400);
        return c.json({ ok: false, error: "missing machine_id query parameter" });
      }

      const stub = getMachineStateDO(machineId, c.env);
      const response = await stub.fetch(
        new Request(`http://dummy/allowlist?machine_id=${encodeURIComponent(machineId)}`),
        {
          method: "GET",
        },
      );

      if (!response.ok) {
        const error = await response.json().catch(() => ({ error: "Unknown error" }));
        return new Response(JSON.stringify(error), {
          status: response.status,
          headers: { "Content-Type": "application/json" },
        });
      }

      return c.json(await response.json());
    } catch (error) {
      console.error("[worker] allowlist get error:", error);
      c.status(500);
      return c.json({ ok: false, error: String(error) });
    }
  });

  app.get("/config", async (c: Context<{ Bindings: WorkerBindings }>) => {
    try {
      const machineId = c.req.query("machine_id");
      if (!machineId) {
        c.status(400);
        return c.json({ ok: false, error: "missing machine_id query parameter" });
      }

      const stub = getMachineStateDO(machineId, c.env);
      const response = await stub.fetch(
        new Request(`http://dummy/config?machine_id=${encodeURIComponent(machineId)}`),
        {
          method: "GET",
        },
      );

      if (!response.ok) {
        const error = await response.json().catch(() => ({ error: "Unknown error" }));
        return new Response(JSON.stringify(error), {
          status: response.status,
          headers: { "Content-Type": "application/json" },
        });
      }

      return c.json(await response.json());
    } catch (error) {
      console.error("[worker] config get error:", error);
      c.status(500);
      return c.json({ ok: false, error: String(error) });
    }
  });

  app.post("/config", async (c: Context<{ Bindings: WorkerBindings }>) => {
    try {
      const body = await c.req.json();
      const machineId = body.machine_id;
      const configPatch = body.config;

      if (!machineId) {
        c.status(400);
        return c.json({ ok: false, error: "missing machine_id" });
      }

      const parsed = MachineConfigSchema.safeParse(configPatch);
      if (!parsed.success) {
        c.status(400);
        return c.json({ ok: false, error: `Invalid config: ${parsed.error.message}` });
      }

      const stub = getMachineStateDO(machineId, c.env);
      const response = await stub.fetch(
        new Request(`http://dummy/config?machine_id=${encodeURIComponent(machineId)}`),
        {
          method: "POST",
          body: JSON.stringify(parsed.data),
        },
      );

      if (!response.ok) {
        const error = await response.json().catch(() => ({ error: "Unknown error" }));
        return new Response(JSON.stringify(error), {
          status: response.status,
          headers: { "Content-Type": "application/json" },
        });
      }

      return c.json(await response.json());
    } catch (error) {
      console.error("[worker] config update error:", error);
      c.status(500);
      return c.json({ ok: false, error: String(error) });
    }
  });
}
