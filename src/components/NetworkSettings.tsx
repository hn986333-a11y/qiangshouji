import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Globe } from "lucide-react";
import { CollapsiblePanel } from "@/components/CollapsiblePanel";
import { Button } from "@/components/ui/button";
import { saveSettings } from "@/lib/store";
import { parseSocksNodes } from "@/lib/socks";
import type { NetworkSettings as Network } from "@/lib/types";

export function NetworkSettings({ network, running, route }: { network: Network; running: boolean; route: string | null }) {
  const socks = network.socks5 ?? { nodes: [], selected: 0, failover: true };
  const [input, setInput] = useState("");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const [statuses, setStatuses] = useState<Record<number, string>>({});
  const [active, setActive] = useState<number | null>(null);
  useEffect(() => { setStatuses({}); }, [network.socks5?.nodes]);
  useEffect(() => {
    if (network.mode !== "socks5") return;
    let alive = true;
    const update = () => { void invoke<number | null>("socks_active_node").then(value => { if (alive) setActive(value); }).catch(() => {}); };
    update(); const timer = setInterval(update, 3000);
    return () => { alive = false; clearInterval(timer); };
  }, [network.mode]);
  const save = async (next: Network) => {
    const error = await saveSettings({ network: next });
    if (!error) setMessage("保存失败，请查看活动日志");
    return error;
  };
  return <CollapsiblePanel id="network" icon={<Globe className="size-4" />} title="查询线路" subtitle={route ?? "内置 SOCKS5 代理管理"} summary={network.mode === "socks5" ? "内置 SOCKS5" : "系统 / 旧线路"}>
    <label className="control-label" htmlFor="network-mode">出口方式</label>
    <select id="network-mode" className="control-surface w-full" value={network.mode} disabled={busy} onChange={event => { void save({ ...network, mode: event.target.value as Network["mode"], socks5: socks }); }}>
      <option value="system">跟随系统代理</option>
      <option value="socks5">内置 SOCKS5（无需 Clash）</option>
      {(network.mode === "pinned" || network.mode === "clash") && <option value={network.mode}>旧版 Clash 配置（建议切换至内置 SOCKS5）</option>}
    </select>
    <p className="mt-2 text-xs text-muted-foreground">Apple 拒绝或限流时暂停库存查询，按 1、2、4 分钟递增退避，最长 1 小时；手动切换和立即重试不会解除退避。{running ? "线路修改将在后续查询生效。" : ""}</p>
    {network.mode === "socks5" && <div className="mt-3 grid gap-3">
      <label htmlFor="socks-import">批量导入（每行 IP:port:user:password）</label>
      <textarea id="socks-import" className="control-surface min-h-24 w-full p-2 font-mono text-xs" value={input} disabled={busy} spellCheck={false} autoComplete="off" placeholder="192.0.2.10:1080:user:password" onChange={e => setInput(e.target.value)} />
      <p className="text-xs text-muted-foreground">支持域名、[IPv6]、空账号密码及密码中的冒号。导入会替换列表。代理凭据随配置以明文保存在本机，请勿分享配置文件。</p>
      <div className="flex gap-2">
        <Button disabled={busy} onClick={async () => {
          try {
            const nodes = parseSocksNodes(input); setBusy(true);
            const result = await save({ ...network, socks5: { ...socks, nodes, selected: 0 } });
            if (!result) return;
            setInput(""); setStatuses({}); setMessage(`已保存 ${nodes.length} 个节点`);
          } catch (error) { setMessage(String(error)); } finally { setBusy(false); }
        }}>导入并保存</Button>
        <Button variant="outline" disabled={busy || !socks.nodes.length} onClick={async () => {
          setBusy(true); setStatuses({});
          try {
            for (let index = 0; index < socks.nodes.length; index++) {
              setStatuses(prev => ({ ...prev, [index]: "检测中…" }));
              try {
                const ms = await invoke<number>("test_socks_node", { node: socks.nodes[index] });
                setStatuses(prev => ({ ...prev, [index]: `隧道可用 ${ms} ms` }));
              } catch (error) { setStatuses(prev => ({ ...prev, [index]: String(error) })); }
            }
          } finally { setBusy(false); }
        }}>检测全部节点</Button>
      </div>
      <label className="flex gap-2 text-sm"><input type="checkbox" checked={socks.failover} disabled={busy} onChange={event => { void save({ ...network, socks5: { ...socks, failover: event.target.checked } }); }} />代理连接或握手失败时使用备用节点</label>
      <p className="text-xs text-muted-foreground">检测仅完成 SOCKS 认证和到 Apple 443 端口的 TCP 隧道，不请求库存，不代表 Apple 允许查询。已建立隧道内的错误不换 IP 重试。</p>
      {socks.nodes.map((node, index) => <div key={index} className="flex items-center justify-between gap-2 rounded border p-2 text-xs">
        <span>{index + 1}. {node.host}:{node.port}{node.username ? "（有认证）" : ""}<br />{statuses[index] ?? "未检测"}{active === index ? " · 当前运行节点" : ""}</span>
        <Button size="sm" variant={socks.selected === index ? "default" : "outline"} disabled={busy} onClick={async () => {
          setBusy(true);
          try {
            if (await save({ ...network, socks5: { ...socks, selected: index } })) {
              await invoke("activate_socks_node"); setActive(index); setMessage(`已切换到节点 ${index + 1}`);
            }
          } catch (error) { setMessage(String(error)); } finally { setBusy(false); }
        }}>{socks.selected === index ? "已设为首选" : "切换并保存"}</Button>
      </div>)}
      <p role="status" className="text-xs">{message}</p>
    </div>}
  </CollapsiblePanel>;
}
