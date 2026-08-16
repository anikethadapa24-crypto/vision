import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import "./GraphExplorer.css";

/** Mirrors src-tauri/src/dto.rs's Graph*Dto types. */
interface GraphNode {
  id: string;
  path: string;
  source: "filesystem" | "browser" | "unspecified";
  created_at_unix_ms: number;
}
interface GraphEdge {
  from_id: string;
  to_id: string;
  weight: number;
}
interface GetGraphResponse {
  nodes: GraphNode[];
  edges: GraphEdge[];
}
interface LaidOutNode extends GraphNode {
  x: number;
  y: number;
}

const WIDTH = 900;
const HEIGHT = 620;
const ITERATIONS = 300;

/**
 * A small, dependency-free force-directed layout: nodes start on a circle
 * (deterministic, not random, so re-running the same graph reproduces the
 * same layout), then repel each other and get pulled together along real
 * edges for `ITERATIONS` steps. Good enough at prototype node counts (tens,
 * not thousands) — UI.SPEC.md §5e explicitly leaves layout as "an
 * engineering detail, not this spec."
 */
function layout(nodes: GraphNode[], edges: GraphEdge[]): LaidOutNode[] {
  const n = nodes.length;
  const positions = nodes.map((node, i) => {
    const angle = (i / Math.max(n, 1)) * Math.PI * 2;
    const r = Math.min(WIDTH, HEIGHT) / 3;
    return { id: node.id, x: WIDTH / 2 + Math.cos(angle) * r, y: HEIGHT / 2 + Math.sin(angle) * r };
  });
  const indexOf = new Map(nodes.map((node, i) => [node.id, i]));
  const vx = new Array(n).fill(0);
  const vy = new Array(n).fill(0);

  for (let iter = 0; iter < ITERATIONS; iter++) {
    for (let i = 0; i < n; i++) {
      for (let j = i + 1; j < n; j++) {
        let dx = positions[i].x - positions[j].x;
        let dy = positions[i].y - positions[j].y;
        const distSq = dx * dx + dy * dy || 0.01;
        const dist = Math.sqrt(distSq);
        const force = 2200 / distSq;
        dx /= dist;
        dy /= dist;
        vx[i] += dx * force;
        vy[i] += dy * force;
        vx[j] -= dx * force;
        vy[j] -= dy * force;
      }
    }
    for (const edge of edges) {
      const i = indexOf.get(edge.from_id);
      const j = indexOf.get(edge.to_id);
      if (i === undefined || j === undefined) continue;
      let dx = positions[j].x - positions[i].x;
      let dy = positions[j].y - positions[i].y;
      const dist = Math.sqrt(dx * dx + dy * dy) || 0.01;
      const force = (dist - 160) * 0.02 * (0.3 + edge.weight);
      dx /= dist;
      dy /= dist;
      vx[i] += dx * force;
      vy[i] += dy * force;
      vx[j] -= dx * force;
      vy[j] -= dy * force;
    }
    for (let i = 0; i < n; i++) {
      vx[i] *= 0.85;
      vy[i] *= 0.85;
      vx[i] += (WIDTH / 2 - positions[i].x) * 0.001;
      vy[i] += (HEIGHT / 2 - positions[i].y) * 0.001;
      positions[i].x = Math.max(48, Math.min(WIDTH - 48, positions[i].x + vx[i]));
      positions[i].y = Math.max(48, Math.min(HEIGHT - 48, positions[i].y + vy[i]));
    }
  }

  const byId = new Map(positions.map((p) => [p.id, p]));
  return nodes.map((node) => ({ ...node, x: byId.get(node.id)!.x, y: byId.get(node.id)!.y }));
}

function nodeLabel(path: string): string {
  if (path.startsWith("http://") || path.startsWith("https://")) {
    try {
      const url = new URL(path);
      return url.hostname + (url.pathname !== "/" ? url.pathname : "");
    } catch {
      return path;
    }
  }
  return path.split(/[\\/]/).pop() ?? path;
}

function openNode(node: GraphNode) {
  if (node.source === "browser") {
    void openUrl(node.path);
  } else {
    void revealItemInDir(node.path);
  }
}

/**
 * Graph Explorer (`docs/UI.SPEC.md` §5e): every ingested document as a node,
 * edges from real cosine similarity between documents (`GetGraph` RPC,
 * `daemon/vision-core/src/graph_query.rs`) — not mock data. Table-view
 * fallback per §8's accessibility rule (any chart-like view ships a
 * non-visual equivalent).
 */
function GraphExplorer() {
  const [data, setData] = useState<GetGraphResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [tableView, setTableView] = useState(false);
  const [loadToken, setLoadToken] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setError(null);
    invoke<GetGraphResponse>("get_graph")
      .then((resp) => {
        if (!cancelled) setData(resp);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [loadToken]);

  const laidOut = useMemo(() => (data ? layout(data.nodes, data.edges) : []), [data]);
  const byId = useMemo(() => new Map(laidOut.map((n) => [n.id, n])), [laidOut]);
  const selected = selectedId ? byId.get(selectedId) : undefined;

  return (
    <div className="graph-root">
      <div className="graph-header">
        <h1 className="graph-title">Graph Explorer</h1>
        <div className="graph-legend">
          <span className="legend-item">
            <span className="dot dot--filesystem" /> Filesystem
          </span>
          <span className="legend-item">
            <span className="dot dot--browser" /> Browser
          </span>
        </div>
        <div className="graph-header-actions">
          <button className="action-button" onClick={() => setLoadToken((t) => t + 1)}>
            Refresh
          </button>
          <button className="action-button" onClick={() => setTableView((v) => !v)}>
            {tableView ? "Graph view" : "Table view"}
          </button>
        </div>
      </div>

      {error && (
        <div className="graph-empty">
          <div>Vision isn't running</div>
          <div className="graph-empty-detail">{error}</div>
        </div>
      )}

      {!error && data && data.nodes.length === 0 && (
        <div className="graph-empty">
          <div>Nothing indexed yet</div>
          <div className="graph-empty-detail">
            Grant a folder to the daemon, or enable the browser extension on a page, then Refresh.
          </div>
        </div>
      )}

      {!error && data && data.nodes.length > 0 && !tableView && (
        <div className="graph-canvas-wrap">
          <svg width={WIDTH} height={HEIGHT} className="graph-canvas">
            {data.edges.map((edge, i) => {
              const a = byId.get(edge.from_id);
              const b = byId.get(edge.to_id);
              if (!a || !b) return null;
              return (
                <line
                  key={i}
                  x1={a.x}
                  y1={a.y}
                  x2={b.x}
                  y2={b.y}
                  className="graph-edge"
                  strokeWidth={1 + edge.weight * 4}
                />
              );
            })}
            {laidOut.map((node) => (
              <g
                key={node.id}
                className={`graph-node${selectedId === node.id ? " graph-node--selected" : ""}`}
                onClick={() => setSelectedId(node.id)}
                onDoubleClick={() => openNode(node)}
              >
                <circle cx={node.x} cy={node.y} r={9} className={`node-dot node-dot--${node.source}`} />
                <text x={node.x} y={node.y + 20} textAnchor="middle" className="node-label">
                  {nodeLabel(node.path)}
                </text>
                <title>{node.path}</title>
              </g>
            ))}
          </svg>
        </div>
      )}

      {!error && data && data.nodes.length > 0 && tableView && (
        <div className="graph-table-wrap">
          <table className="graph-table">
            <thead>
              <tr>
                <th>Type</th>
                <th>Path</th>
                <th>Indexed</th>
                <th>Connections</th>
              </tr>
            </thead>
            <tbody>
              {data.nodes.map((node) => {
                const connections = data.edges.filter(
                  (e) => e.from_id === node.id || e.to_id === node.id,
                ).length;
                return (
                  <tr key={node.id} onClick={() => setSelectedId(node.id)}>
                    <td>
                      <span className={`dot dot--${node.source}`} /> {node.source}
                    </td>
                    <td className="graph-table-path">{node.path}</td>
                    <td>{new Date(node.created_at_unix_ms).toLocaleString()}</td>
                    <td>{connections}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      {selected && (
        <div className="graph-detail">
          <div className="graph-detail-title">{nodeLabel(selected.path)}</div>
          <div className="graph-detail-path">{selected.path}</div>
          <div className="graph-detail-meta">
            {selected.source} · {new Date(selected.created_at_unix_ms).toLocaleString()}
          </div>
          <button className="action-button" onClick={() => openNode(selected)}>
            Open source
          </button>
        </div>
      )}
    </div>
  );
}

export default GraphExplorer;
