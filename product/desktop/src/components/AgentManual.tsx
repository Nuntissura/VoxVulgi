import { useState } from "react";
import manual from "../lib/agentManual.json";

export function AgentManual() {
  const [query, setQuery] = useState("");
  const matches = (value: unknown) => JSON.stringify(value).toLowerCase().includes(query.toLowerCase());
  return <section id="options-manual" aria-label={manual.title}>
    <label style={{ display: "block", marginBottom: 6 }} htmlFor="agent-manual-search">Search the user manual</label>
    <input style={{ width: "100%", boxSizing: "border-box", marginBottom: 12 }} id="agent-manual-search" type="search" value={query}
      data-agent-action-id="manual.search" data-agent-effect-class="read_only" data-agent-input-kind="text"
      onChange={event => setQuery(event.target.value)} placeholder="Navigation, downloads, recovery…" />
    <p>This manual and the backend capability catalog share the same product source. Read <code>GET /agent/capabilities</code> for current availability.</p>
    {manual.topics.filter(matches).map(topic => <section key={topic.title}><h3>{topic.title}</h3><p>{topic.body}</p></section>)}
    <h3>Navigation, inspection and diagnostics</h3>
    <div style={{ overflowX: "auto" }}><table><thead><tr><th>Route</th><th>Use</th></tr></thead><tbody>
      {manual.endpoints.filter(matches).map(endpoint => <tr key={endpoint.path}><td><code>{endpoint.method} {endpoint.path}</code></td><td>{endpoint.description}</td></tr>)}
    </tbody></table></div>
    <h3>Backend commands</h3>
    <p>Send JSON to <code>POST /agent/command</code> with <code>bridge_token</code>, <code>actor_id</code>, and <code>command</code>. Mutations also require <code>operation_id</code>. Never put the token in logs or screenshots.</p>
    {manual.commands.filter(matches).map(command => <details key={command.name}><summary><code>{command.name}</code> — {command.read_only ? "Read" : "Change"}</summary><p>{command.description}</p><pre style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>{JSON.stringify(command.input, null, 2)}</pre></details>)}
  </section>;
}
