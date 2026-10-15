import {
  anthropic,
  gemini,
  openai,
  parseSSE
} from "./chunk-MRHKJ2FZ.js";
import {
  defaultStorage
} from "./chunk-YCVJQ7U3.js";
import {
  __publicField
} from "./chunk-MPTP2EWW.js";

// src/ai/byok.ts
var LABELS = {
  anthropic: "Anthropic (Claude)",
  openai: "OpenAI",
  gemini: "Google Gemini",
  "openai-compatible": "OpenAI-compatible (OpenRouter, Groq, Ollama\u2026)"
};
function read(storage, key) {
  try {
    let raw = storage.getItem(key);
    return raw ? JSON.parse(raw) : void 0;
  } catch {
    return;
  }
}
var byok = /* @__PURE__ */ Object.assign(function(options = {}) {
  let storage = options.storage ?? defaultStorage(), key = options.key ?? "qikbar:byok";
  return {
    id: "byok",
    get model() {
      return read(storage, key)?.model;
    },
    async *stream(req) {
      let s = read(storage, key);
      if (!s?.apiKey && s?.vendor !== "openai-compatible") {
        yield { type: "error", error: "Add your AI API key first: search for \u201CAI Settings\u201D in the command bar." }, yield { type: "done", message: { role: "assistant", content: "" } };
        return;
      }
      yield* (s.vendor === "anthropic" ? anthropic({ apiKey: s.apiKey, model: s.model || void 0 }) : s.vendor === "gemini" ? gemini({ apiKey: s.apiKey, model: s.model || void 0 }) : openai({ apiKey: s.apiKey || void 0, model: s.model || void 0, baseURL: s.baseURL || void 0 })).stream(req);
    }
  };
}, {
  /** An extension with an "AI Settings" command where users enter their own key. */
  settings: settingsExtension
});
function settingsExtension(options = {}) {
  let storage = options.storage ?? defaultStorage(), key = options.key ?? "qikbar:byok", vendors = options.vendors ?? Object.keys(LABELS);
  return {
    name: "qikbar-ai-settings",
    title: "AI",
    icon: "key",
    commands: [
      {
        name: "settings",
        title: "AI Settings",
        subtitle: "Use your own API key",
        keywords: ["api key", "openai", "anthropic", "claude", "gemini", "model", "byok"],
        icon: "key",
        view: () => {
          let current = read(storage, key);
          return {
            type: "form",
            title: "AI Settings",
            submitLabel: "Save",
            fields: [
              {
                name: "vendor",
                label: "Provider",
                type: "select",
                options: vendors.map((v) => ({ title: LABELS[v], value: v })),
                defaultValue: current?.vendor ?? vendors[0]
              },
              {
                name: "apiKey",
                label: "API key",
                type: "password",
                placeholder: "Stored only in this browser",
                defaultValue: current?.apiKey ?? "",
                description: "Your key never leaves your browser except to call the provider directly."
              },
              { name: "model", label: "Model", placeholder: "Leave empty for the default", defaultValue: current?.model ?? "" },
              { name: "baseURL", label: "Base URL", placeholder: "Only for OpenAI-compatible APIs", defaultValue: current?.baseURL ?? "" }
            ],
            onSubmit(values, ctx) {
              storage.setItem(key, JSON.stringify(values)), ctx.toast({ title: "AI settings saved", style: "success" });
            }
          };
        }
      },
      {
        name: "clear",
        title: "Remove AI API Key",
        icon: "trash",
        keywords: ["byok", "api key"],
        searchOnly: !0,
        async run(ctx) {
          if (!await ctx.confirm({ title: "Remove your API key?", destructive: !0, confirmLabel: "Remove" })) return !1;
          storage.removeItem(key), ctx.toast({ title: "API key removed", style: "success" });
        }
      }
    ]
  };
}

// src/ai/mcp.ts
var MCP_PROTOCOL_VERSION = "2025-06-18", MCPError = class extends Error {
  constructor(message, code) {
    super(message);
    __publicField(this, "code", code);
    this.name = "MCPError";
  }
}, MCPClient = class {
  constructor(options) {
    __publicField(this, "options", options);
    __publicField(this, "session");
    __publicField(this, "protocol", MCP_PROTOCOL_VERSION);
    __publicField(this, "ready");
    __publicField(this, "negotiated", !1);
    __publicField(this, "nextId", 1);
    __publicField(this, "serverInfo");
  }
  async headers() {
    let h = this.options.headers, extra = typeof h == "function" ? await h() : h ?? {};
    return {
      "content-type": "application/json",
      accept: "application/json, text/event-stream",
      ...this.session ? { "mcp-session-id": this.session } : {},
      ...this.negotiated ? { "mcp-protocol-version": this.protocol } : {},
      ...extra
    };
  }
  async post(body, signal, expectId) {
    let res = await (this.options.fetch ?? globalThis.fetch)(this.options.url, {
      method: "POST",
      headers: await this.headers(),
      body: JSON.stringify(body),
      signal,
      credentials: this.options.credentials ?? "same-origin"
    }), session = res.headers.get("mcp-session-id");
    if (session && (this.session = session), res.status === 404 && this.session && expectId != null)
      throw this.session = void 0, this.ready = void 0, this.negotiated = !1, new MCPError("MCP session expired", 404);
    if (!res.ok && res.status !== 202) {
      let text = await res.text().catch(() => "");
      throw new MCPError(`MCP server responded ${res.status}${text ? `: ${text.slice(0, 200)}` : ""}`, res.status);
    }
    if (expectId == null || res.status === 202) return;
    if ((res.headers.get("content-type") ?? "").includes("text/event-stream")) {
      for await (let ev of parseSSE(res)) {
        if (!ev.data) continue;
        let msg;
        try {
          msg = JSON.parse(ev.data);
        } catch {
          continue;
        }
        for (let m of Array.isArray(msg) ? msg : [msg]) if (m.id === expectId) return m;
      }
      throw new MCPError("MCP stream ended without a response");
    }
    let json = await res.json(), list = Array.isArray(json) ? json : [json];
    return list.find((m) => m.id === expectId) ?? list[0];
  }
  async request(method, params, signal, retry = !0) {
    method !== "initialize" && await this.connect(signal);
    let id = this.nextId++, res;
    try {
      res = await this.post({ jsonrpc: "2.0", id, method, ...params ? { params } : {} }, signal, id);
    } catch (err) {
      if (retry && err instanceof MCPError && err.code === 404 && method !== "initialize") return this.request(method, params, signal, !1);
      throw err;
    }
    if (!res) throw new MCPError(`No response to ${method}`);
    if (res.error) throw new MCPError(res.error.message, res.error.code);
    return res.result;
  }
  /** Initialize the session (once). */
  connect(signal) {
    return this.ready ?? (this.ready = (async () => {
      let result = await this.request(
        "initialize",
        { protocolVersion: MCP_PROTOCOL_VERSION, capabilities: {}, clientInfo: { name: "qikbar", version: "0.2.1" } },
        signal
      );
      result?.protocolVersion && (this.protocol = result.protocolVersion), this.negotiated = !0, this.serverInfo = result?.serverInfo, await this.post({ jsonrpc: "2.0", method: "notifications/initialized" }, signal);
    })()), this.ready.catch(() => {
      this.ready = void 0;
    }), this.ready;
  }
  /** List every tool, following pagination. */
  async listTools(signal) {
    let tools = [], cursor;
    for (let page = 0; page < 20; page++) {
      let res = await this.request("tools/list", cursor ? { cursor } : void 0, signal);
      if (tools.push(...res?.tools ?? []), cursor = res?.nextCursor, !cursor) break;
    }
    return tools;
  }
  /** Call a tool and return its result as text for the model. */
  async callTool(name, args, signal) {
    let res = await this.request("tools/call", { name, arguments: args }, signal), text = contentToText(res?.content, res?.structuredContent);
    if (res?.isError) throw new MCPError(text || `Tool ${name} failed`);
    return text;
  }
  /** End the session (best effort). */
  async close() {
    if (!this.session) return;
    let doFetch = this.options.fetch ?? globalThis.fetch;
    try {
      await doFetch(this.options.url, { method: "DELETE", headers: await this.headers(), credentials: this.options.credentials ?? "same-origin" });
    } catch {
    }
    this.session = void 0, this.ready = void 0, this.negotiated = !1;
  }
};
function contentToText(content, structured) {
  let parts = [];
  for (let c of content ?? [])
    c.type === "text" && c.text != null ? parts.push(c.text) : c.type === "resource" && c.resource?.text ? parts.push(c.resource.text) : c.type === "resource_link" ? parts.push(`[${c.name ?? c.uri}](${c.uri})`) : (c.type === "image" || c.type === "audio") && parts.push(`[${c.type}${c.mimeType ? ` ${c.mimeType}` : ""}]`);
  return !parts.length && structured !== void 0 ? JSON.stringify(structured) : parts.join(`

`);
}
function nameFromUrl(url) {
  try {
    return (new URL(url, typeof location < "u" ? location.href : "http://localhost").host.replace(/^(www|api|mcp)\./, "").split(".")[0] || "mcp").replace(/[^a-zA-Z0-9_-]/g, "_");
  } catch {
    return "mcp";
  }
}
function mcp(options) {
  let client = new MCPClient(options), name = (options.name ?? nameFromUrl(options.url)).replace(/[^a-zA-Z0-9_-]/g, "_"), cache, ttl = options.ttl ?? 5 * 6e4, shouldConfirm = (info) => {
    let c = options.confirm;
    return typeof c == "function" ? c(info) : typeof c == "boolean" ? c : info.annotations?.readOnlyHint !== !0;
  }, load = async (signal) => (await client.listTools(signal)).filter((t) => (!options.include || options.include.includes(t.name)) && !options.exclude?.includes(t.name)).map(
    (info) => ({
      name: info.name,
      description: info.description || info.annotations?.title || info.title || info.name,
      parameters: info.inputSchema ?? { type: "object", properties: {} },
      confirm: shouldConfirm(info) ? () => `Allow ${info.annotations?.title ?? info.title ?? info.name} (${name})?` : !1,
      run: (args, ctx) => client.callTool(info.name, args ?? {}, ctx.signal)
    })
  );
  return {
    name,
    client,
    refresh() {
      cache = void 0;
    },
    tools(signal) {
      if (!cache || Date.now() - cache.at > ttl) {
        let entry = { at: Date.now(), tools: load(signal) };
        cache = entry, entry.tools.catch(() => {
          cache === entry && (cache = void 0);
        });
      }
      return cache.tools;
    }
  };
}

export {
  byok,
  MCP_PROTOCOL_VERSION,
  MCPError,
  MCPClient,
  mcp
};
//# sourceMappingURL=chunk-24EBWTCF.js.map
