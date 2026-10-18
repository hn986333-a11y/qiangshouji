import type { NetworkSettings } from "./types.ts";
type Node = NetworkSettings["socks5"]["nodes"][number];
/** Atomic import: never partially overwrite the user's list on a malformed line. */
export function parseSocksNodes(text: string): Node[] {
  const nodes: Node[] = [];
  const seen = new Set<string>();
  for (const [index, raw] of text.split(/\r?\n/).entries()) {
    const line = raw.trimStart();
    if (!line.trim() || line.startsWith("#")) continue;
    const match = line.match(/^(\[[^\]]+\]|[^:]+):(\d+):([^:]*):(.*)$/);
    if (!match) throw new Error(`第 ${index + 1} 行格式错误，应为 IP:port:user:password`);
    const rawHost = match[1]!;
    const rawPort = match[2]!;
    const username = match[3]!;
    const password = match[4]!;
    const host = rawHost.replace(/^\[|\]$/g, "");
    const port = Number(rawPort);
    const bytes = new TextEncoder();
    if (!Number.isInteger(port) || port < 1 || port > 65535 || /[\s/@?#]/.test(host) || bytes.encode(username).length > 255 || bytes.encode(password).length > 255 || /[\r\n\0]/.test(username + password)) {
      throw new Error(`第 ${index + 1} 行地址、端口或认证信息无效`);
    }
    const node = { host, port, username, password };
    const key = JSON.stringify(node);
    if (!seen.has(key)) { nodes.push(node); seen.add(key); }
    if (nodes.length > 100) throw new Error("一次最多导入 100 个节点");
  }
  if (!nodes.length) throw new Error("请输入至少一个节点");
  return nodes;
}
