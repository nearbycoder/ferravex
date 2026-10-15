// src/ai/sse.ts
async function* lines(response) {
  if (!response.body) return;
  let reader = response.body.getReader(), decoder = new TextDecoder(), buffer = "";
  try {
    for (; ; ) {
      let { value, done } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: !0 });
      let nl;
      for (; (nl = buffer.indexOf(`
`)) >= 0; ) {
        let line = buffer.slice(0, nl).replace(/\r$/, "");
        buffer = buffer.slice(nl + 1), yield line;
      }
    }
    buffer += decoder.decode(), buffer && (yield buffer);
  } finally {
    reader.releaseLock();
  }
}
async function* parseSSE(response) {
  let event, data = [];
  for await (let line of lines(response))
    line === "" ? (data.length && (yield { event, data: data.join(`
`) }), event = void 0, data = []) : line.startsWith("data:") ? data.push(line.slice(line[5] === " " ? 6 : 5)) : line.startsWith("event:") && (event = line.slice(6).trim());
  data.length && (yield { event, data: data.join(`
`) });
}
async function* parseNDJSON(response) {
  for await (let line of lines(response))
    line.trim() && (yield JSON.parse(line));
}
async function errorFromResponse(response, provider) {
  let detail = "";
  try {
    let text = await response.text();
    try {
      let json = JSON.parse(text);
      detail = json?.error?.message ?? json?.message ?? json?.error ?? text;
    } catch {
      detail = text;
    }
  } catch {
  }
  return new Error(`${provider} ${response.status}: ${String(detail || response.statusText).slice(0, 300)}`);
}

// src/ai/providers.ts
var callCounter = 0, newCallId = () => `call_${Date.now().toString(36)}${(callCounter++).toString(36)}`;
function parseArgs(json) {
  if (!json.trim()) return { args: {} };
  try {
    let parsed = JSON.parse(json);
    return parsed && typeof parsed == "object" && !Array.isArray(parsed) ? { args: parsed } : { args: {}, error: "Tool arguments must be a JSON object." };
  } catch {
    return { args: {}, error: `Invalid JSON arguments: ${json.slice(0, 200)}` };
  }
}
var ANTHROPIC_EFFORT_MODELS = /claude-(opus|sonnet|haiku|fable|mythos)-5|claude-opus-4-[5-8]/, ANTHROPIC_FALLBACK_MODELS = /^claude-(opus-5|opus-5-5|sonnet-5-5|fable-5-1)$/;
function anthropic(options = {}) {
  let model = options.model ?? "claude-opus-5-5", baseURL = (options.baseURL ?? "https://api.anthropic.com").replace(/\/$/, ""), doFetch = options.fetch ?? ((...args) => fetch(...args));
  function toMessages(messages) {
    let out = [];
    for (let m of messages)
      if (m.role === "user") out.push({ role: "user", content: m.content });
      else if (m.role === "assistant")
        if (m.raw?.provider === "anthropic") out.push({ role: "assistant", content: m.raw.data });
        else {
          let blocks = [];
          m.content && blocks.push({ type: "text", text: m.content });
          for (let c of m.toolCalls ?? []) blocks.push({ type: "tool_use", id: c.id, name: c.name, input: c.args });
          out.push({ role: "assistant", content: blocks.length ? blocks : m.content || "\u2026" });
        }
      else {
        let block = { type: "tool_result", tool_use_id: m.toolCallId, content: m.content, ...m.isError ? { is_error: !0 } : {} }, last = out[out.length - 1];
        last?.role === "user" && Array.isArray(last.content) && last.content[0]?.type === "tool_result" ? last.content.push(block) : out.push({ role: "user", content: [block] });
      }
    return out;
  }
  return {
    id: "anthropic",
    model,
    async *stream(req) {
      let headers = {
        "content-type": "application/json",
        "anthropic-version": "2023-06-01",
        ...options.headers
      };
      options.apiKey && (headers["x-api-key"] = options.apiKey, headers["anthropic-dangerous-direct-browser-access"] = "true");
      let body = {
        model,
        max_tokens: req.maxTokens ?? options.maxTokens ?? 16e3,
        stream: !0,
        messages: toMessages(req.messages)
      };
      req.system && (body.system = req.system), req.tools?.length && (body.tools = req.tools.map((t) => ({ name: t.name, description: t.description, input_schema: t.parameters, eager_input_streaming: !0 })));
      let effort = options.effort === void 0 ? ANTHROPIC_EFFORT_MODELS.test(model) ? "low" : !1 : options.effort;
      effort && (body.output_config = { effort }), options.fallbacks !== !1 && ANTHROPIC_FALLBACK_MODELS.test(model) && !options.baseURL && (body.fallbacks = "default", headers["anthropic-beta"] = [headers["anthropic-beta"], "server-side-fallback-2026-07-01"].filter(Boolean).join(","));
      let res = await doFetch(`${baseURL}/v1/messages`, { method: "POST", headers, body: JSON.stringify(body), signal: req.signal });
      if (!res.ok) throw await errorFromResponse(res, "Anthropic");
      let blocks = {}, stopReason, text = "";
      for await (let msg of parseSSE(res)) {
        if (!msg.data || msg.data === "[DONE]") continue;
        let ev = JSON.parse(msg.data);
        switch (ev.type) {
          case "content_block_start": {
            let block = { ...ev.content_block };
            block.type === "tool_use" && (block._json = ""), blocks[ev.index] = block, block.type === "text" && block.text && (text += block.text, yield { type: "text", text: block.text });
            break;
          }
          case "content_block_delta": {
            let block = blocks[ev.index], d = ev.delta;
            if (!block) break;
            d.type === "text_delta" ? (block.text = (block.text ?? "") + d.text, text += d.text, yield { type: "text", text: d.text }) : d.type === "input_json_delta" ? block._json += d.partial_json : d.type === "thinking_delta" ? block.thinking = (block.thinking ?? "") + d.thinking : d.type === "signature_delta" && (block.signature = d.signature);
            break;
          }
          case "content_block_stop": {
            let block = blocks[ev.index];
            if (block?.type === "tool_use") {
              let { args, error } = parseArgs(block._json);
              block.input = args, error && (block._error = error), delete block._json;
            }
            break;
          }
          case "message_delta":
            ev.delta?.stop_reason && (stopReason = ev.delta.stop_reason);
            break;
          case "error":
            throw new Error(`Anthropic: ${ev.error?.message ?? "stream error"}`);
        }
      }
      let ordered = Object.keys(blocks).map(Number).sort((a, b) => a - b).map((i) => blocks[i]), toolCalls = [];
      if (stopReason !== "max_tokens" && stopReason !== "refusal")
        for (let b of ordered) {
          if (b.type !== "tool_use") continue;
          let call = { id: b.id, name: b.name, args: b.input ?? {} };
          b._error && (call.error = b._error), toolCalls.push(call), yield { type: "tool_call", call };
        }
      for (let b of ordered) delete b._error;
      stopReason === "refusal" && (yield { type: "error", error: "The model declined to answer this request." }), yield {
        type: "done",
        stopReason,
        message: { role: "assistant", content: text, toolCalls: toolCalls.length ? toolCalls : void 0, raw: { provider: "anthropic", data: ordered } }
      };
    }
  };
}
function openai(options = {}) {
  let model = options.model ?? "gpt-6-luna", baseURL = (options.baseURL ?? "https://api.openai.com/v1").replace(/\/$/, ""), doFetch = options.fetch ?? ((...args) => fetch(...args));
  function toMessages(messages, system) {
    let out = [];
    system && out.push({ role: "system", content: system });
    for (let m of messages)
      m.role === "user" ? out.push({ role: "user", content: m.content }) : m.role === "assistant" ? out.push({
        role: "assistant",
        content: m.content || null,
        ...m.toolCalls?.length ? { tool_calls: m.toolCalls.map((c) => ({ id: c.id, type: "function", function: { name: c.name, arguments: JSON.stringify(c.args) } })) } : {}
      }) : out.push({ role: "tool", tool_call_id: m.toolCallId, content: m.content });
    return out;
  }
  return {
    id: options.id ?? "openai",
    model,
    tools: options.tools ?? !0,
    async *stream(req) {
      let body = { model, stream: !0, messages: toMessages(req.messages, req.system) }, maxTokens = req.maxTokens ?? options.maxTokens;
      maxTokens && (body.max_completion_tokens = maxTokens), req.tools?.length && options.tools !== !1 && (body.tools = req.tools.map((t) => ({ type: "function", function: { name: t.name, description: t.description, parameters: t.parameters } })));
      let res = await doFetch(`${baseURL}/chat/completions`, {
        method: "POST",
        headers: {
          "content-type": "application/json",
          ...options.apiKey ? { authorization: `Bearer ${options.apiKey}` } : {},
          ...options.headers
        },
        body: JSON.stringify(body),
        signal: req.signal
      });
      if (!res.ok) throw await errorFromResponse(res, options.id ?? "OpenAI");
      let text = "", stopReason, calls = [];
      for await (let msg of parseSSE(res)) {
        if (!msg.data || msg.data === "[DONE]") continue;
        let chunk = JSON.parse(msg.data);
        if (chunk.error) throw new Error(`${options.id ?? "OpenAI"}: ${chunk.error.message ?? chunk.error}`);
        let choice = chunk.choices?.[0];
        if (!choice) continue;
        let delta = choice.delta ?? {};
        typeof delta.content == "string" && delta.content && (text += delta.content, yield { type: "text", text: delta.content });
        for (let tc of delta.tool_calls ?? []) {
          let i = tc.index ?? 0;
          calls[i] ?? (calls[i] = { id: tc.id ?? newCallId(), name: "", json: "" }), tc.id && (calls[i].id = tc.id), tc.function?.name && (calls[i].name += tc.function.name), tc.function?.arguments && (calls[i].json += tc.function.arguments);
        }
        choice.finish_reason && (stopReason = choice.finish_reason);
      }
      let toolCalls = [];
      if (stopReason !== "length" && stopReason !== "content_filter")
        for (let c of calls.filter(Boolean)) {
          let { args, error } = parseArgs(c.json), call = { id: c.id, name: c.name, args, ...error ? { error } : {} };
          toolCalls.push(call), yield { type: "tool_call", call };
        }
      stopReason === "content_filter" && (yield { type: "error", error: "The response was blocked by the provider\u2019s content filter." }), yield { type: "done", stopReason, message: { role: "assistant", content: text, toolCalls: toolCalls.length ? toolCalls : void 0 } };
    }
  };
}
function openaiCompatible(options) {
  return openai({ id: "openai-compatible", ...options });
}
function ollama(options = { model: "llama3.2" }) {
  return openai({ id: "ollama", baseURL: options.baseURL ?? "http://localhost:11434/v1", model: options.model, tools: options.tools });
}
function geminiSchema(schema) {
  let { additionalProperties: _a, $schema: _s, default: _d, ...rest } = schema, out = { ...rest };
  return rest.properties && (out.properties = Object.fromEntries(Object.entries(rest.properties).map(([k, v]) => [k, geminiSchema(v)]))), rest.items && (out.items = geminiSchema(rest.items)), out;
}
function gemini(options = {}) {
  let model = options.model ?? "gemini-flash-latest", baseURL = (options.baseURL ?? "https://generativelanguage.googleapis.com/v1beta").replace(/\/$/, ""), doFetch = options.fetch ?? ((...args) => fetch(...args));
  function toContents(messages) {
    let out = [];
    for (let m of messages)
      if (m.role === "user") out.push({ role: "user", parts: [{ text: m.content }] });
      else if (m.role === "assistant")
        if (m.raw?.provider === "gemini") out.push({ role: "model", parts: m.raw.data });
        else {
          let parts = [];
          m.content && parts.push({ text: m.content });
          for (let c of m.toolCalls ?? []) parts.push({ functionCall: { name: c.name, args: c.args } });
          out.push({ role: "model", parts: parts.length ? parts : [{ text: "" }] });
        }
      else {
        let part = { functionResponse: { name: m.toolName ?? "tool", response: m.isError ? { error: m.content } : { result: m.content } } }, last = out[out.length - 1];
        last?.role === "user" && last.parts[0]?.functionResponse ? last.parts.push(part) : out.push({ role: "user", parts: [part] });
      }
    return out;
  }
  return {
    id: "gemini",
    model,
    async *stream(req) {
      let body = { contents: toContents(req.messages) };
      req.system && (body.systemInstruction = { parts: [{ text: req.system }] }), req.tools?.length && (body.tools = [
        {
          functionDeclarations: req.tools.map((t) => ({
            name: t.name,
            description: t.description,
            ...t.parameters?.properties && Object.keys(t.parameters.properties).length ? { parameters: geminiSchema(t.parameters) } : {}
          }))
        }
      ]);
      let maxTokens = req.maxTokens ?? options.maxTokens;
      maxTokens && (body.generationConfig = { maxOutputTokens: maxTokens });
      let res = await doFetch(`${baseURL}/models/${encodeURIComponent(model)}:streamGenerateContent?alt=sse`, {
        method: "POST",
        headers: { "content-type": "application/json", ...options.apiKey ? { "x-goog-api-key": options.apiKey } : {}, ...options.headers },
        body: JSON.stringify(body),
        signal: req.signal
      });
      if (!res.ok) throw await errorFromResponse(res, "Gemini");
      let text = "", stopReason, parts = [], toolCalls = [];
      for await (let msg of parseSSE(res)) {
        if (!msg.data) continue;
        let chunk = JSON.parse(msg.data);
        if (chunk.error) throw new Error(`Gemini: ${chunk.error.message}`);
        if (chunk.promptFeedback?.blockReason) {
          yield { type: "error", error: `Blocked by Gemini (${chunk.promptFeedback.blockReason}).` };
          continue;
        }
        let cand = chunk.candidates?.[0];
        if (cand) {
          for (let part of cand.content?.parts ?? [])
            if (part.functionCall) {
              parts.push(part);
              let call = { id: part.functionCall.id ?? newCallId(), name: part.functionCall.name, args: part.functionCall.args ?? {} };
              toolCalls.push(call);
            } else if (typeof part.text == "string") {
              let last = parts[parts.length - 1];
              !part.thought && last && typeof last.text == "string" && !last.thought && !last.thoughtSignature && !part.thoughtSignature ? last.text += part.text : parts.push({ ...part }), !part.thought && part.text && (text += part.text, yield { type: "text", text: part.text });
            } else parts.push(part);
          cand.finishReason && (stopReason = cand.finishReason);
        }
      }
      if (stopReason !== "MAX_TOKENS") for (let call of toolCalls) yield { type: "tool_call", call };
      stopReason === "SAFETY" && (yield { type: "error", error: "The response was blocked by Gemini safety filters." }), yield {
        type: "done",
        stopReason,
        message: { role: "assistant", content: text, toolCalls: toolCalls.length && stopReason !== "MAX_TOKENS" ? toolCalls : void 0, raw: { provider: "gemini", data: parts } }
      };
    }
  };
}
var browserAI = /* @__PURE__ */ Object.assign(
  function() {
    return {
      id: "browser",
      model: "on-device",
      tools: !1,
      async *stream(req) {
        let LM = globalThis.LanguageModel;
        if (!LM) throw new Error("Built-in AI is not available in this browser.");
        let history = req.messages.filter((m) => m.role !== "tool"), last = history.pop(), initialPrompts = [
          ...req.system ? [{ role: "system", content: req.system }] : [],
          ...history.map((m) => ({ role: m.role, content: m.content }))
        ], session = await LM.create({ initialPrompts, signal: req.signal }), total = "";
        try {
          let stream = session.promptStreaming(last?.content ?? "", { signal: req.signal });
          for await (let chunk of stream) {
            let delta = chunk.startsWith(total) && total ? chunk.slice(total.length) : chunk;
            total += delta, delta && (yield { type: "text", text: delta });
          }
        } finally {
          session.destroy?.();
        }
        yield { type: "done", message: { role: "assistant", content: total } };
      }
    };
  },
  {
    /** Whether the on-device model can be used in this browser. */
    async isAvailable() {
      let LM = globalThis.LanguageModel;
      if (!LM?.availability) return !1;
      try {
        return await LM.availability() !== "unavailable";
      } catch {
        return !1;
      }
    }
  }
);
function proxy(options) {
  let opts = typeof options == "string" ? { url: options } : options, doFetch = opts.fetch ?? ((...args) => fetch(...args));
  return {
    id: "proxy",
    async *stream(req) {
      let extra = typeof opts.headers == "function" ? await opts.headers() : opts.headers, res = await doFetch(opts.url, {
        method: "POST",
        headers: { "content-type": "application/json", accept: "application/x-ndjson", ...extra },
        credentials: opts.credentials ?? "same-origin",
        body: JSON.stringify({ messages: req.messages, system: req.system, tools: req.tools, maxTokens: req.maxTokens }),
        signal: req.signal
      });
      if (!res.ok) throw await errorFromResponse(res, "AI proxy");
      yield* parseNDJSON(res);
    }
  };
}

// src/ai/embeddings.ts
async function postJSON(url, body, headers, signal, f = globalThis.fetch) {
  let res = await f(url, { method: "POST", headers: { "content-type": "application/json", ...headers }, body: JSON.stringify(body), signal });
  if (!res.ok) {
    let text = await res.text().catch(() => "");
    throw new Error(`Embeddings request failed (${res.status})${text ? `: ${text.slice(0, 200)}` : ""}`);
  }
  return res.json();
}
function openaiEmbeddings(options = {}) {
  let base = (options.baseURL ?? "https://api.openai.com/v1").replace(/\/$/, ""), dimensions = options.dimensions === void 0 ? 256 : options.dimensions;
  return async (texts, { signal }) => (await postJSON(
    `${base}/embeddings`,
    { model: options.model ?? "text-embedding-3-small", input: texts, ...dimensions ? { dimensions } : {} },
    { ...options.apiKey ? { authorization: `Bearer ${options.apiKey}` } : {}, ...options.headers },
    signal,
    options.fetch
  )).data.sort((a, b) => a.index - b.index).map((d) => d.embedding);
}
function ollamaEmbeddings(options = {}) {
  let host = (options.host ?? "http://localhost:11434").replace(/\/$/, "");
  return async (texts, { signal }) => (await postJSON(`${host}/api/embed`, { model: options.model ?? "nomic-embed-text", input: texts }, {}, signal, options.fetch)).embeddings;
}
function geminiEmbeddings(options) {
  let model = options.model ?? "gemini-embedding-001";
  return async (texts, { signal }) => (await postJSON(
    `https://generativelanguage.googleapis.com/v1beta/models/${model}:batchEmbedContents`,
    {
      requests: texts.map((text) => ({
        model: `models/${model}`,
        content: { parts: [{ text }] },
        outputDimensionality: options.dimensions ?? 256
      }))
    },
    { "x-goog-api-key": options.apiKey },
    signal,
    options.fetch
  )).embeddings.map((e) => e.values);
}
function proxyEmbeddings(options) {
  return async (texts, { signal }) => (await postJSON(options.url, { texts }, options.headers ?? {}, signal, options.fetch)).embeddings;
}
function cachedEmbeddings(embed, maxEntries = 2e3) {
  let cache = /* @__PURE__ */ new Map();
  return async (texts, opts) => {
    let missing = [...new Set(texts.filter((t) => !cache.has(t)))];
    if (missing.length) {
      let vecs = await embed(missing, opts);
      for (missing.forEach((t, i) => {
        vecs[i] && cache.set(t, vecs[i]);
      }); cache.size > maxEntries; ) cache.delete(cache.keys().next().value);
    }
    return texts.map((t) => cache.get(t) ?? []);
  };
}

export {
  parseSSE,
  parseNDJSON,
  anthropic,
  openai,
  openaiCompatible,
  ollama,
  gemini,
  browserAI,
  proxy,
  openaiEmbeddings,
  ollamaEmbeddings,
  geminiEmbeddings,
  proxyEmbeddings,
  cachedEmbeddings
};
//# sourceMappingURL=chunk-MRHKJ2FZ.js.map
