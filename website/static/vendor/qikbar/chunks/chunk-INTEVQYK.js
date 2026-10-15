// src/core/define.ts
function list(view) {
  return { ...view, type: "list" };
}
function grid(view) {
  return { ...view, type: "grid" };
}
function detail(view) {
  return { ...view, type: "detail" };
}
function form(view) {
  return { ...view, type: "form" };
}
function chat(view = {}) {
  return { ...view, type: "chat" };
}
var action = {
  /** Copy text to the clipboard. */
  copy(content, options = {}) {
    return {
      title: "Copy to Clipboard",
      labelKey: "copyToClipboard",
      icon: "copy",
      shortcut: "mod+c",
      ...options,
      async run(ctx) {
        let ok = await ctx.copy(typeof content == "function" ? content() : content), l = ctx.labels;
        ctx.toast({ title: ok ? l?.copied ?? "Copied to clipboard" : l?.copyFailed ?? "Copy failed", style: ok ? "success" : "failure" });
      }
    };
  },
  /** Open an external URL in a new tab. */
  open(url, options = {}) {
    return {
      title: "Open in Browser",
      labelKey: "openInBrowser",
      icon: "external",
      ...options,
      run: (ctx) => ctx.openUrl(url)
    };
  },
  /** Navigate within your app. */
  navigate(href, options = {}) {
    return {
      title: "Open",
      labelKey: "open",
      icon: "arrow-right",
      ...options,
      run: (ctx) => ctx.navigate(href)
    };
  },
  /** Navigate in a new tab. */
  navigateNewTab(href, options = {}) {
    return {
      title: "Open in New Tab",
      labelKey: "openInNewTab",
      icon: "external",
      shortcut: "mod+enter",
      ...options,
      run: (ctx) => ctx.navigate(href, { newTab: !0 })
    };
  },
  /** Push a view. */
  push(title, view, options = {}) {
    return {
      title,
      icon: "chevron-right",
      ...options,
      run: (ctx) => typeof view == "function" ? view(ctx) : view
    };
  },
  /** Ask the AI about something. */
  ask(prompt, options = {}) {
    return {
      title: "Ask AI",
      icon: "sparkles",
      shortcut: "mod+j",
      ...options,
      run: () => ({ type: "chat", prompt })
    };
  }
};
function defineExtension(extension) {
  return extension;
}
function defineCommands(commands) {
  return commands;
}
function defineSource(source) {
  return source;
}
function defineTool(tool) {
  return tool;
}

export {
  list,
  grid,
  detail,
  form,
  chat,
  action,
  defineExtension,
  defineCommands,
  defineSource,
  defineTool
};
//# sourceMappingURL=chunk-INTEVQYK.js.map
