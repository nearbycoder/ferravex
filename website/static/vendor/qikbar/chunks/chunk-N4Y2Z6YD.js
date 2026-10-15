// src/ai/agent.ts
function createAIClient(provider, defaultSystem) {
  async function* stream(prompt, options = {}) {
    for await (let ev of provider.stream({
      messages: [{ role: "user", content: prompt }],
      system: options.system ?? defaultSystem,
      signal: options.signal
    }))
      if (ev.type === "text") yield ev.text;
      else if (ev.type === "error") throw new Error(ev.error);
  }
  return {
    provider,
    stream,
    async ask(prompt, options) {
      let out = "";
      for await (let t of stream(prompt, options)) out += t;
      return out;
    }
  };
}
function stringify(value) {
  if (value === void 0) return "Done.";
  if (typeof value == "string") return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}
var MAX_RESULT = 12e3;
async function* runAgent(options) {
  let { provider, system, signal } = options, tools = provider.tools === !1 ? [] : options.tools ?? [], history = [...options.messages], maxSteps = options.maxSteps ?? 6;
  for (let step = 0; step < maxSteps; step++) {
    let calls = [], assistant, text = "";
    for await (let ev of provider.stream({
      messages: [...history],
      system,
      tools: tools.length && step < maxSteps - 1 ? tools.map((t) => t.definition) : void 0,
      signal
    }))
      ev.type === "text" ? (text += ev.text, yield ev) : ev.type === "tool_call" ? calls.push(ev.call) : ev.type === "error" ? yield ev : ev.type === "done" && (assistant = ev.message);
    if (assistant ?? (assistant = { role: "assistant", content: text, toolCalls: calls.length ? calls : void 0 }), history.push(assistant), yield { type: "message", message: assistant }, !calls.length || signal.aborted) return;
    for (let call of calls) {
      let tool = tools.find((t) => t.definition.name === call.name), label = tool?.label?.(call.args) ?? call.name;
      yield { type: "tool_start", call, label };
      let content, isError = !1;
      if (call.error)
        content = call.error, isError = !0;
      else if (!tool)
        content = `Unknown tool "${call.name}".`, isError = !0;
      else
        try {
          content = stringify(await tool.run(call.args, signal));
        } catch (err) {
          content = err instanceof Error ? err.message : String(err), isError = !0;
        }
      content.length > MAX_RESULT && (content = `${content.slice(0, MAX_RESULT)}\u2026 (truncated)`);
      let message = { role: "tool", toolCallId: call.id, toolName: call.name, content, ...isError ? { isError } : {} };
      if (history.push(message), yield { type: "message", message }, yield { type: "tool_end", call, ok: !isError, result: content }, signal.aborted) return;
    }
  }
}

export {
  createAIClient,
  runAgent
};
//# sourceMappingURL=chunk-N4Y2Z6YD.js.map
