export type AgentUiAuditRequest = {
  limit?: number;
  include_offscreen?: boolean;
};

export type AgentUiActionRequest = {
  audit_id?: string;
  action?: "click" | "scroll_into_view" | "scroll_content";
  scroll_top?: number;
};

type AgentUiRect = {
  x: number;
  y: number;
  width: number;
  height: number;
};

export type AgentUiElement = {
  audit_id: string;
  id: string | null;
  test_id: string | null;
  tag: string;
  role: string;
  name: string;
  text_preview: string;
  class_name: string;
  type: string | null;
  rendered: boolean;
  in_viewport: boolean;
  disabled: boolean;
  checked: boolean | null;
  selected: boolean | null;
  expanded: boolean | null;
  value_summary: string | null;
  rect: AgentUiRect;
  safe_actions: string[];
  dom_path: string;
};

export type AgentUiAuditResult = {
  timestamp_ms: number;
  viewport: { width: number; height: number };
  content_scroll: { top: number; height: number; client_height: number } | null;
  total_candidates: number;
  returned_elements: number;
  truncated: boolean;
  missing_accessible_name_count: number;
  elements: AgentUiElement[];
};

let nextAuditId = 1;

function compactText(value: string | null | undefined, maxLength: number): string {
  const compact = (value ?? "").replace(/\s+/g, " ").trim();
  return compact.length > maxLength ? `${compact.slice(0, maxLength)}…` : compact;
}

function referencedText(element: HTMLElement, attribute: string): string {
  const ids = (element.getAttribute(attribute) ?? "").split(/\s+/).filter(Boolean);
  return compactText(
    ids
      .map((id) => document.getElementById(id)?.textContent ?? "")
      .filter(Boolean)
      .join(" "),
    240,
  );
}

function implicitRole(element: HTMLElement): string {
  const tag = element.tagName.toLowerCase();
  if (tag === "button" || tag === "summary") return "button";
  if (tag === "a" && element.hasAttribute("href")) return "link";
  if (tag === "select") return element.hasAttribute("multiple") ? "listbox" : "combobox";
  if (tag === "textarea") return "textbox";
  if (tag === "input") {
    const type = (element.getAttribute("type") ?? "text").toLowerCase();
    if (type === "checkbox") return "checkbox";
    if (type === "radio") return "radio";
    if (type === "search") return "searchbox";
    if (type === "range") return "slider";
    if (type === "number") return "spinbutton";
    if (type === "button" || type === "submit" || type === "reset") return "button";
    return "textbox";
  }
  if (/^h[1-6]$/.test(tag)) return "heading";
  if (tag === "nav") return "navigation";
  if (tag === "form") return "form";
  if (tag === "table") return "table";
  if (tag === "details") return "group";
  if (tag === "section") return "region";
  return "generic";
}

function accessibleName(element: HTMLElement, role: string): string {
  const ariaLabel = compactText(element.getAttribute("aria-label"), 240);
  if (ariaLabel) return ariaLabel;
  const labelledBy = referencedText(element, "aria-labelledby");
  if (labelledBy) return labelledBy;
  if (
    element instanceof HTMLInputElement ||
    element instanceof HTMLSelectElement ||
    element instanceof HTMLTextAreaElement
  ) {
    const labelText = compactText(
      Array.from(element.labels ?? [])
        .map((label) => label.textContent ?? "")
        .join(" "),
      240,
    );
    if (labelText) return labelText;
    const placeholder = compactText(element.getAttribute("placeholder"), 240);
    if (placeholder) return placeholder;
  }
  if (
    role === "button" ||
    role === "link" ||
    role === "heading" ||
    role === "tab" ||
    role === "option" ||
    element.tagName.toLowerCase() === "summary"
  ) {
    const contentName = compactText(element.textContent, 240);
    if (contentName) return contentName;
  }
  const title = compactText(element.getAttribute("title"), 240);
  if (title) return title;
  return "";
}

function isRendered(element: HTMLElement): boolean {
  const style = window.getComputedStyle(element);
  return (
    style.display !== "none" &&
    style.visibility !== "hidden" &&
    style.opacity !== "0" &&
    element.getClientRects().length > 0
  );
}

function auditIdFor(element: HTMLElement): string {
  const current = element.dataset.vvAgentAuditId?.trim();
  if (current) return current;
  const assigned = `vv-audit-${nextAuditId++}`;
  element.dataset.vvAgentAuditId = assigned;
  return assigned;
}

function stateBoolean(element: HTMLElement, attribute: string): boolean | null {
  const value = element.getAttribute(attribute);
  if (value === null) return null;
  return value === "true";
}

function domPath(element: HTMLElement): string {
  const parts: string[] = [];
  let current: HTMLElement | null = element;
  while (current && parts.length < 5) {
    let part = current.tagName.toLowerCase();
    if (current.id) {
      part += `#${current.id}`;
      parts.unshift(part);
      break;
    }
    const testId = current.dataset.testid;
    if (testId) part += `[data-testid="${testId}"]`;
    else if (current.parentElement) {
      const siblings = Array.from(current.parentElement.children).filter(
        (candidate) => candidate.tagName === current?.tagName,
      );
      if (siblings.length > 1) part += `:nth-of-type(${siblings.indexOf(current) + 1})`;
    }
    parts.unshift(part);
    current = current.parentElement;
  }
  return parts.join(" > ");
}

function safeValueSummary(element: HTMLElement): string | null {
  if (element instanceof HTMLSelectElement) {
    return compactText(element.selectedOptions.item(0)?.textContent ?? "", 120);
  }
  if (element instanceof HTMLInputElement) {
    const type = (element.type || "text").toLowerCase();
    if (type === "checkbox" || type === "radio") return element.checked ? "checked" : "not_checked";
    const sensitive = `${element.id} ${element.name} ${element.getAttribute("aria-label") ?? ""}`.toLowerCase();
    if (type === "password" || /cookie|token|password|secret/.test(sensitive)) return "<redacted>";
    return element.value ? `${element.value.length} characters` : "empty";
  }
  if (element instanceof HTMLTextAreaElement) {
    return element.value ? `${element.value.length} characters` : "empty";
  }
  return null;
}

export function classifySafeAgentActions(
  tag: string,
  role: string,
  hasStructuralState: boolean,
  explicitlySafe: boolean,
): string[] {
  const actions = ["scroll_into_view"];
  if (tag === "summary" || role === "tab" || hasStructuralState || explicitlySafe) actions.push("click");
  return actions;
}

function describeElement(element: HTMLElement): AgentUiElement {
  const rect = element.getBoundingClientRect();
  const tag = element.tagName.toLowerCase();
  const role = element.getAttribute("role")?.trim() || implicitRole(element);
  const rendered = isRendered(element);
  const inViewport =
    rendered &&
    rect.bottom > 0 &&
    rect.right > 0 &&
    rect.top < window.innerHeight &&
    rect.left < window.innerWidth;
  const ariaChecked = stateBoolean(element, "aria-checked");
  const ariaSelected = stateBoolean(element, "aria-selected");
  const nativeChecked =
    element instanceof HTMLInputElement && (element.type === "checkbox" || element.type === "radio")
      ? element.checked
      : null;
  return {
    audit_id: auditIdFor(element),
    id: element.id || null,
    test_id: element.dataset.testid || null,
    tag,
    role,
    name: accessibleName(element, role),
    text_preview: compactText(element.textContent, 180),
    class_name: compactText(element.className, 180),
    type: element.getAttribute("type"),
    rendered,
    in_viewport: inViewport,
    disabled:
      (element instanceof HTMLButtonElement ||
        element instanceof HTMLInputElement ||
        element instanceof HTMLSelectElement ||
        element instanceof HTMLTextAreaElement) &&
      element.disabled,
    checked: ariaChecked ?? nativeChecked,
    selected: ariaSelected ?? stateBoolean(element, "aria-pressed"),
    expanded:
      stateBoolean(element, "aria-expanded") ??
      (element instanceof HTMLDetailsElement ? element.open : null),
    value_summary: safeValueSummary(element),
    rect: {
      x: Math.round(rect.x),
      y: Math.round(rect.y),
      width: Math.round(rect.width),
      height: Math.round(rect.height),
    },
    safe_actions: classifySafeAgentActions(
      tag,
      role,
      element.hasAttribute("aria-pressed") ||
        element.hasAttribute("aria-expanded") ||
        (role === "option" && element.hasAttribute("aria-selected")),
      element.dataset.agentSafeAction === "true",
    ),
    dom_path: domPath(element),
  };
}

export function buildAgentUiAudit(request: AgentUiAuditRequest = {}): AgentUiAuditResult {
  const root = document.querySelector<HTMLElement>(".content") ?? document.body;
  const requestedLimit = Number.isFinite(request.limit) ? Number(request.limit) : 700;
  const limit = Math.max(1, Math.min(1500, Math.trunc(requestedLimit)));
  const candidates = Array.from(
    root.querySelectorAll<HTMLElement>(
      [
        "h1",
        "h2",
        "h3",
        "nav",
        "form",
        "section",
        "table",
        "details",
        "summary",
        "button",
        "a[href]",
        "input",
        "select",
        "textarea",
        "[role]",
        "[data-testid]",
      ].join(","),
    ),
  );
  const unique = Array.from(new Set(candidates));
  const described = unique.map(describeElement);
  const eligible = request.include_offscreen === false
    ? described.filter((element) => element.in_viewport)
    : described.filter((element) => element.rendered);
  const elements = eligible.slice(0, limit);
  const content = document.querySelector<HTMLElement>(".content");
  return {
    timestamp_ms: Date.now(),
    viewport: { width: window.innerWidth, height: window.innerHeight },
    content_scroll: content
      ? {
          top: Math.round(content.scrollTop),
          height: Math.round(content.scrollHeight),
          client_height: Math.round(content.clientHeight),
        }
      : null,
    total_candidates: eligible.length,
    returned_elements: elements.length,
    truncated: eligible.length > elements.length,
    missing_accessible_name_count: elements.filter(
      (element) =>
        ["button", "link", "textbox", "searchbox", "combobox", "checkbox", "radio", "tab", "option"].includes(
          element.role,
        ) && !element.name,
    ).length,
    elements,
  };
}

export function performAgentUiAction(request: AgentUiActionRequest): Record<string, unknown> {
  const action = request.action;
  if (action === "scroll_content") {
    const content = document.querySelector<HTMLElement>(".content");
    if (!content) throw new Error("content scroll surface is unavailable");
    const requested = Number(request.scroll_top ?? 0);
    content.scrollTop = Math.max(0, Math.min(content.scrollHeight, Number.isFinite(requested) ? requested : 0));
    return { action, scroll_top: Math.round(content.scrollTop) };
  }

  const auditId = request.audit_id?.trim();
  if (!auditId) throw new Error("audit_id is required");
  const target = document.querySelector<HTMLElement>(`[data-vv-agent-audit-id="${CSS.escape(auditId)}"]`);
  if (!target) throw new Error(`audit element not found: ${auditId}; run ui_audit again`);
  const before = describeElement(target);

  if (action === "scroll_into_view") {
    target.scrollIntoView({ behavior: "auto", block: "center", inline: "nearest" });
  } else if (action === "click") {
    if (!before.safe_actions.includes("click")) {
      throw new Error(`refused unsafe click for ${before.tag} "${before.name || before.text_preview}"`);
    }
    if (before.disabled) throw new Error(`refused disabled control: ${before.name || before.audit_id}`);
    target.click();
  } else {
    throw new Error(`unsupported UI action: ${String(action)}`);
  }

  return {
    action,
    target_before: before,
  };
}
