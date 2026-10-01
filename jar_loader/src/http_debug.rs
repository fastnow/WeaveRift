//! 127.0.0.1 调试台。
//!
//! ★ 安全三件事：
//!   1. 只绑 127.0.0.1
//!   2. 每次启动随机 token
//!   3. 校验 Host 头（防 DNS rebinding）
//! ★ CORS 绝不设 *

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;

use tiny_http::{Header, Response, Server};

use crate::logger;
use crate::state;

pub static PORT: u16 = 11451;
static TOKEN: OnceLock<String> = OnceLock::new();

pub static HUD_ON: AtomicBool = AtomicBool::new(true);
pub static SAMPLE_HZ: AtomicU64 = AtomicU64::new(20);

pub fn token() -> &'static str {
    TOKEN.get_or_init(|| {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let pid = std::process::id() as u64;
        let mut x = n ^ (pid.wrapping_mul(0x9E3779B97F4A7C15));
        x ^= x >> 33;
        x = x.wrapping_mul(0xFF51AFD7ED558CCD);
        x ^= x >> 33;
        format!("{:016x}", x)
    })
}

fn parse_query(url: &str) -> (String, HashMap<String, String>) {
    let mut it = url.splitn(2, '?');
    let path = it.next().unwrap_or("/").to_string();
    let mut map = HashMap::new();
    if let Some(q) = it.next() {
        for kv in q.split('&') {
            let mut p = kv.splitn(2, '=');
            let k = p.next().unwrap_or("").to_string();
            let v = p.next().unwrap_or("").to_string();
            map.insert(k, v);
        }
    }
    (path, map)
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "")
}

fn ct(kind: &str) -> Header {
    Header::from_bytes(&b"Content-Type"[..], kind.as_bytes()).unwrap()
}

fn security_headers(port: u16) -> Vec<Header> {
    let origin = format!("http://127.0.0.1:{}", port);
    vec![
        Header::from_bytes(&b"X-Content-Type-Options"[..], &b"nosniff"[..]).unwrap(),
        Header::from_bytes(&b"X-Frame-Options"[..], &b"DENY"[..]).unwrap(),
        Header::from_bytes(&b"Access-Control-Allow-Origin"[..], origin.as_bytes()).unwrap(),
        Header::from_bytes(&b"Cache-Control"[..], &b"no-store"[..]).unwrap(),
    ]
}

fn is_local_host(host: &str) -> bool {
    let h = host.to_ascii_lowercase();
    h.starts_with("127.0.0.1")
        || h.starts_with("localhost")
        || h.starts_with("[::1]")
        || h.starts_with("::1")
}

/// 由状态机在渲染线程里调用
pub fn start() {
    let addr = format!("127.0.0.1:{}", PORT);
    let server = match Server::http(&addr) {
        Ok(s) => s,
        Err(e) => {
            logger::error(&format!("调试台启动失败: {}", e));
            return;
        }
    };

    let t = token().to_string();
    logger::info(&format!("调试台 http://127.0.0.1:{}/?token={}", PORT, t));

    // 写到临时文件方便外部工具读取
    let mut p = std::env::temp_dir();
    p.push("WeaveRift");
    let _ = std::fs::create_dir_all(&p);
    p.push("debug-url.txt");
    let _ = std::fs::write(
        p,
        format!("http://127.0.0.1:{}/?token={}\n", PORT, t),
    );

    std::thread::spawn(move || {
        for mut req in server.incoming_requests() {
            let url = req.url().to_string();
            let (path, q) = parse_query(&url);

            // ── 安全校验 ──
            let host_ok = req
                .headers()
                .iter()
                .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case("Host"))
                .map(|h| is_local_host(&String::from_utf8_lossy(h.value.as_bytes()).to_string()))
                .unwrap_or(false);

            let tok_ok = q.get("token").map(|v| v == &t).unwrap_or(false);

            if !host_ok || !tok_ok {
                let body = if !host_ok { "403: bad host" } else { "401: bad token" };
                let r = Response::from_string(body).with_status_code(403);
                let _ = req.respond(r);
                continue;
            }

            let mut body = String::new();
            if req.method() == &tiny_http::Method::Post {
                let _ = req.as_reader().read_to_string(&mut body);
            }

            let (payload, kind) = route(&path, &q, &body);
            let mut resp = Response::from_string(payload)
                .with_header(ct(kind))
                .with_status_code(200);
            for h in security_headers(PORT) {
                resp = resp.with_header(h);
            }
            let _ = req.respond(resp);
        }
    });
}

fn route(path: &str, q: &HashMap<String, String>, body: &str) -> (String, &'static str) {
    match path {
        "/" => (console_html(), "text/html; charset=utf-8"),

        "/api/status" => {
            let json = format!(
                r#"{{"state":"{}","hud":{},"hz":{},"seq":{},"vm":{},"ctx":{},"registered":{}}}"#,
                state::name(state::current()),
                HUD_ON.load(Ordering::Relaxed),
                SAMPLE_HZ.load(Ordering::Relaxed),
                logger::latest_seq(),
                crate::jni_bridge::has_vm(),
                crate::gl_ctx::valid(),
                crate::jni_bridge::is_registered(),
            );
            (json, "application/json; charset=utf-8")
        }

        "/api/logs" => {
            let since: u64 = q.get("since").and_then(|v| v.parse().ok()).unwrap_or(0);
            let recs = logger::since(since);
            let mut out = String::from("{\"logs\":[");
            for (i, (seq, ms, lvl, msg)) in recs.iter().enumerate() {
                if i > 0 { out.push(','); }
                out.push_str(&format!(
                    r#"{{"seq":{},"ms":{},"lvl":{},"msg":"{}"}}"#,
                    seq, ms, lvl, json_escape(msg)
                ));
            }
            out.push_str("]}");
            (out, "application/json; charset=utf-8")
        }

        "/api/command" => {
            let cmd = if body.is_empty() {
                q.get("cmd").cloned().unwrap_or_default()
            } else {
                body.to_string()
            };
            let out = match cmd.as_str() {
                "hud_on" => { HUD_ON.store(true, Ordering::Relaxed); "HUD on".into() }
                "hud_off" => { HUD_ON.store(false, Ordering::Relaxed); "HUD off".into() }

                "rescan" => {
                   let jr = crate::jni_bridge::send_command("rescan")
                        .unwrap_or_else(|e| format!("err: {}", e));
                    crate::state::recover();
                    format!("rescan requested, state recovered; java: {}", jr)
                }

                "recover" => {
                    crate::state::recover();
                    "state recovered".into()
                }

                "status" => crate::jni_bridge::send_command("status")
                    .unwrap_or_else(|e| format!("err: {}", e)),

                "error" => crate::jni_bridge::send_command("error")
                    .unwrap_or_else(|e| format!("err: {}", e)),

                "unhook" => {
                    crate::request_unhook();
                    "unhook requested".into()
                }

                other => match other.strip_prefix("hz=") {
                    Some(v) => {
                        let hz: u64 = v.parse().unwrap_or(20).clamp(1, 120);
                        SAMPLE_HZ.store(hz, Ordering::Relaxed);
                        crate::set_sample_hz(hz);
                        format!("sample hz = {}", hz)
                    }
                    None => format!("unknown cmd: {}", other),
                },
            };
            logger::info(&format!("command '{}' -> {}", cmd, out));
            (out, "text/plain; charset=utf-8")
        }

        _ => (String::from("404"), "text/plain; charset=utf-8"),
    }
}

fn console_html() -> String {
    // 同目录有 console.html 就用它
    let mut p = std::env::current_exe().unwrap_or_default();
    p.pop();
    p.push("console.html");
    if let Ok(s) = std::fs::read_to_string(&p) {
        return s;
    }
    FALLBACK_HTML.to_string()
}

const FALLBACK_HTML: &str = r#"<!doctype html><meta charset=utf-8>
<title>WeaveRift Debug</title>
<body style="background:#111;color:#ddd;font:13px Consolas,monospace;margin:0">
<div style="padding:12px;border-bottom:1px solid #333">
  <b>WeaveRift</b> &nbsp;
  <button onclick="cmd('hud_on')">HUD on</button>
  <button onclick="cmd('hud_off')">HUD off</button>
  <button onclick="cmd('recover')">恢复状态机</button>
  <button onclick="cmd('rescan')">重扫映射</button>
  <button onclick="cmd('status')">Java 状态</button>
  <span id=st></span>
</div>
<pre id=log style="padding:12px;white-space:pre-wrap;margin:0"></pre>
<script>
let last=0, tok=new URLSearchParams(location.search).get('token')||'';
const COL=['#888','#9cf','#7f7','#fc7','#f77'];
async function poll(){
  try{
    const s=await (await fetch('/api/status?token='+tok)).json();
    document.getElementById('st').textContent=
      'state='+s.state+' hud='+s.hud+' vm='+s.vm+' ctx='+s.ctx+' reg='+s.registered;
    const l=await (await fetch('/api/logs?token='+tok+'&since='+last)).json();
    const box=document.getElementById('log');
    for(const r of l.logs){
      last=Math.max(last,r.seq);
      const d=document.createElement('div');
      d.style.color=COL[r.lvl]||'#ddd';
      d.textContent=r.msg; box.appendChild(d);
    }
    while(box.childElementCount>2000) box.removeChild(box.firstChild);
    box.scrollTop=box.scrollHeight;
  }catch(e){}
}
async function cmd(c){
  await fetch('/api/command?token='+tok+'&cmd='+c,{method:'POST'}); poll();
}
poll(); setInterval(poll,300);
</script>"#;