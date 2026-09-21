#!/usr/bin/env python3
"""Generate trans-kun v3 artboards (.dc.html) + canvas.json."""
import json, os, datetime

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "project")
os.makedirs(ROOT, exist_ok=True)

# ---------------------------------------------------------------- icons
ICONS = {
 "mic": '<path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" y1="19" x2="12" y2="22"/>',
 "speaker": '<polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M15.54 8.46a5 5 0 0 1 0 7.07"/><path d="M19.07 4.93a10 10 0 0 1 0 14.14"/>',
 "monitor": '<rect x="2" y="3" width="20" height="14" rx="2"/><line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="17" x2="12" y2="21"/>',
 "file": '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/>',
 "search": '<circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>',
 "tag": '<path d="M20.59 13.41l-7.17 7.17a2 2 0 0 1-2.83 0L2 12V2h10l8.59 8.59a2 2 0 0 1 0 2.82z"/><line x1="7" y1="7" x2="7.01" y2="7"/>',
 "settings": '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>',
 "home": '<path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/>',
 "play": '<polygon points="5 3 19 12 5 21 5 3"/>',
 "pause": '<rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/>',
 "download": '<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/>',
 "copy": '<rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>',
 "refresh": '<polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>',
 "x": '<line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>',
 "check": '<polyline points="20 6 9 17 4 12"/>',
 "alert": '<path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>',
 "wifi-off": '<line x1="1" y1="1" x2="23" y2="23"/><path d="M16.72 11.06A10.94 10.94 0 0 1 19 12.55"/><path d="M5 12.55a10.94 10.94 0 0 1 5.17-2.39"/><path d="M10.71 5.05A16 16 0 0 1 22.58 9"/><path d="M1.42 9a15.91 15.91 0 0 1 4.7-2.88"/><path d="M8.53 16.11a6 6 0 0 1 6.95 0"/><line x1="12" y1="20" x2="12.01" y2="20"/>',
 "wifi": '<path d="M5 12.55a11 11 0 0 1 14.08 0"/><path d="M1.42 9a16 16 0 0 1 21.16 0"/><path d="M8.53 16.11a6 6 0 0 1 6.95 0"/><line x1="12" y1="20" x2="12.01" y2="20"/>',
 "chevron-down": '<polyline points="6 9 12 15 18 9"/>',
 "chevron-left": '<polyline points="15 18 9 12 15 6"/>',
 "chevron-up": '<polyline points="18 15 12 9 6 15"/>',
 "more": '<circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/><circle cx="5" cy="12" r="1"/>',
 "help": '<circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><line x1="12" y1="17" x2="12.01" y2="17"/>',
 "eye": '<path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/>',
 "languages": '<path d="M5 8l6 6"/><path d="M4 14l6-6 2-3"/><path d="M2 5h12"/><path d="M7 2h1"/><path d="M22 22l-5-10-5 10"/><path d="M14 18h6"/>',
 "radio": '<circle cx="12" cy="12" r="2"/><path d="M16.24 7.76a6 6 0 0 1 0 8.49m-8.48-.01a6 6 0 0 1 0-8.49m11.31-2.82a10 10 0 0 1 0 14.14m-14.14 0a10 10 0 0 1 0-14.14"/>',
 "upload": '<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/>',
 "external": '<path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/><polyline points="15 3 21 3 21 9"/><line x1="10" y1="14" x2="21" y2="3"/>',
 "folder": '<path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>',
 "trash": '<polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>',
 "edit": '<path d="M12 20h9"/><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z"/>',
 "shield": '<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>',
 "key": '<path d="M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0l3 3L22 7l-3-3m-3.5 3.5L19 4"/>',
 "stop": '<rect x="5" y="5" width="14" height="14" rx="2"/>',
 "columns": '<rect x="3" y="3" width="18" height="18" rx="2"/><line x1="12" y1="3" x2="12" y2="21"/>',
 "memo": '<path d="M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9z"/><path d="M19 16l.8 2.2L22 19l-2.2.8L19 22l-.8-2.2L16 19l2.2-.8z"/>',
 "clock": '<circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/>',
 "image": '<rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/>',
 "flag": '<path d="M4 15s1-1 4-1 5 2 8 2 4-1 4-1V3s-1 1-4 1-5-2-8-2-4 1-4 1z"/><line x1="4" y1="22" x2="4" y2="15"/>',
 "wave": '<path d="M2 12h2l2-5 3 10 3-14 3 14 3-10 2 5h2"/>',
 "globe": '<circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>',
 "cloud": '<path d="M18 10h-1.26A8 8 0 1 0 9 20h9a5 5 0 0 0 0-10z"/>',
 "laptop": '<rect x="3" y="4" width="18" height="12" rx="2"/><path d="M2 20h20"/>',
 "arrow-right": '<line x1="5" y1="12" x2="19" y2="12"/><polyline points="12 5 19 12 12 19"/>',
 "sliders": '<line x1="4" y1="21" x2="4" y2="14"/><line x1="4" y1="10" x2="4" y2="3"/><line x1="12" y1="21" x2="12" y2="12"/><line x1="12" y1="8" x2="12" y2="3"/><line x1="20" y1="21" x2="20" y2="16"/><line x1="20" y1="12" x2="20" y2="3"/><line x1="1" y1="14" x2="7" y2="14"/><line x1="9" y1="8" x2="15" y2="8"/><line x1="17" y1="16" x2="23" y2="16"/>',
 "database": '<ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M21 12c0 1.66-4 3-9 3s-9-1.34-9-3"/><path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5"/>',
 "activity": '<polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/>',
 "info": '<circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/>',
 "scissors": '<circle cx="6" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><line x1="20" y1="4" x2="8.12" y2="15.88"/><line x1="14.47" y1="14.48" x2="20" y2="20"/><line x1="8.12" y1="8.12" x2="12" y2="12"/>',
 "zap": '<polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/>',
 "grip": '<circle cx="9" cy="6" r="1"/><circle cx="15" cy="6" r="1"/><circle cx="9" cy="12" r="1"/><circle cx="15" cy="12" r="1"/><circle cx="9" cy="18" r="1"/><circle cx="15" cy="18" r="1"/>',
}

def ic(name, size=18, color="currentColor", sw=1.75):
    return (f'<svg aria-hidden="true" width="{size}" height="{size}" viewBox="0 0 24 24" fill="none" '
            f'stroke="{color}" stroke-width="{sw}" stroke-linecap="round" stroke-linejoin="round" '
            f'style="flex: none; width: {size}px; height: {size}px;">{ICONS[name]}</svg>')

# ---------------------------------------------------------------- shared css
FONT_LINK = '<link href="https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;500;600;700&amp;family=IBM+Plex+Mono:wght@400;500&amp;display=swap" rel="stylesheet">'
CSS = """
body{margin:0;font-family:"IBM Plex Sans",-apple-system,"Segoe UI","Hiragino Sans","Yu Gothic UI",sans-serif;background:#F6F6F2;color:#171A1F;-webkit-font-smoothing:antialiased;font-size:14px;line-height:1.5}
a{color:#0F766E}a:hover{color:#0B5D57}
*{box-sizing:border-box}
.mono{font-family:"IBM Plex Mono",ui-monospace,Menlo,monospace;font-variant-numeric:tabular-nums}
.btn{display:inline-flex;align-items:center;justify-content:center;gap:8px;height:36px;padding:0 14px;border-radius:8px;font:inherit;font-size:14px;font-weight:500;cursor:pointer;border:1px solid transparent;text-decoration:none;white-space:nowrap;transition:background-color 150ms ease,color 150ms ease,border-color 150ms ease}
.btn-primary{background:#0F766E;color:#FFFFFF}.btn-primary:hover{background:#0B5D57;color:#FFFFFF}
.btn-secondary{background:#FFFFFF;color:#171A1F;border-color:#CFD4DA}.btn-secondary:hover{background:#F1F2EE;color:#171A1F}
.btn-ghost{background:transparent;color:#3C4551}.btn-ghost:hover{background:#ECEDE8;color:#171A1F}
.btn-danger{background:#B42318;color:#FFFFFF}.btn-danger:hover{background:#8E1C13;color:#FFFFFF}
.btn-danger-soft{background:#FFFFFF;color:#B42318;border-color:#F0B8B3}.btn-danger-soft:hover{background:#FDF1F0;color:#B42318}
.btn-lg{height:44px;padding:0 20px;font-size:15px;border-radius:10px}
.btn-sm{height:30px;padding:0 10px;font-size:13px}
.btn-icon{width:36px;padding:0}.btn-icon.btn-sm{width:30px}
.btn[aria-disabled="true"]{opacity:.45;cursor:not-allowed}
.btn:focus-visible,.input:focus-visible,.select:focus-visible,.seg-btn:focus-visible,.nav-item:focus-visible,.row:focus-visible,textarea:focus-visible{outline:2px solid #0F766E;outline-offset:2px}
.input,.select{height:36px;padding:0 12px;border:1px solid #CFD4DA;border-radius:8px;font:inherit;font-size:14px;background:#FFFFFF;color:#171A1F;width:100%}
.select{appearance:none;-webkit-appearance:none;padding-right:32px;background-image:url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='16' height='16' viewBox='0 0 24 24' fill='none' stroke='%235B6470' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><polyline points='6 9 12 15 18 9'/></svg>");background-repeat:no-repeat;background-position:right 10px center;cursor:pointer}
textarea.input{height:auto;padding:10px 12px;resize:none;line-height:1.55}
.label{font-size:13px;font-weight:600;color:#3C4551;display:flex;align-items:center;gap:6px}
.help{font-size:12px;color:#5B6470;line-height:1.5}
.card{background:#FFFFFF;border:1px solid #E1E4E8;border-radius:12px}
.chip{display:inline-flex;align-items:center;gap:6px;height:26px;padding:0 10px;border-radius:999px;font-size:12px;font-weight:500;background:#ECEDE8;color:#3C4551;border:1px solid transparent;cursor:pointer}
.chip-on{background:#E6F3F1;color:#0B5D57;border-color:#B7DDD8}
.badge{display:inline-flex;align-items:center;gap:4px;height:20px;padding:0 7px;border-radius:6px;font-size:11px;font-weight:600;letter-spacing:.02em;white-space:nowrap}
.badge-memo{background:#E0F2F1;color:#0B5D57}
.badge-audio{background:#E8EEF7;color:#1E4E9B}
.badge-partial{background:#FDF0DC;color:#8A4B0A}
.badge-recover{background:#F0EAFB;color:#5B2FA3}
.badge-live{background:#FDE8E8;color:#A11E1E}
.badge-file{background:#ECEDE8;color:#3C4551}
.badge-token{background:#FFF7E6;color:#8A4B0A;border:1px solid #F5D9A8}
.seg{display:grid;grid-template-columns:56px 1fr;gap:12px;padding:8px 12px;border-radius:8px;cursor:pointer;align-items:start}
.seg:hover{background:#F1F2EE}
.seg-active{background:#E6F3F1}
.ts{font-family:"IBM Plex Mono",ui-monospace,Menlo,monospace;font-variant-numeric:tabular-nums;font-size:12px;color:#5B6470;padding-top:3px}
.speaker{font-size:11px;font-weight:700;color:#0F766E;margin-right:6px;letter-spacing:.03em}
.seg-text{font-size:14px;line-height:1.55;color:#171A1F}
.mark{background:#FDE68A;border-radius:3px;padding:0 2px}
.nav-item{display:flex;align-items:center;gap:10px;height:38px;padding:0 12px;border-radius:8px;font-size:14px;color:#3C4551;text-decoration:none;cursor:pointer;font-weight:500}
.nav-item:hover{background:#ECEDE8;color:#171A1F}
.nav-item-active{background:#E6F3F1;color:#0B5D57;font-weight:600}.nav-item-active:hover{background:#E6F3F1;color:#0B5D57}
.row{display:grid;grid-template-columns:minmax(0,1fr) 120px 88px 84px 300px 36px;gap:12px;align-items:center;height:48px;padding:0 12px;border-top:1px solid #EDEFF1;cursor:pointer;text-decoration:none;color:#171A1F}
.row:hover{background:#F7F7F4}
.th{display:grid;grid-template-columns:minmax(0,1fr) 120px 88px 84px 300px 36px;gap:12px;align-items:center;height:36px;padding:0 12px;font-size:12px;font-weight:600;color:#5B6470;text-transform:uppercase;letter-spacing:.04em}
.chip-sm{height:22px;padding:0 8px;font-size:11px}
.seg-btn{height:30px;padding:0 12px;border:0;background:transparent;border-radius:6px;font:inherit;font-size:13px;font-weight:500;color:#3C4551;cursor:pointer}
.seg-btn-on{background:#FFFFFF;color:#0B5D57;font-weight:600;box-shadow:0 1px 2px rgba(17,24,39,.08)}
.segmented{display:inline-flex;gap:2px;padding:3px;background:#ECEDE8;border-radius:8px}
.status{display:inline-flex;align-items:center;gap:8px;height:30px;padding:0 10px;border-radius:999px;font-size:13px;font-weight:600}
.status-rec{background:#FDE8E8;color:#A11E1E}
.status-ok{background:#E0F2F1;color:#0B5D57}
.status-warn{background:#FDF0DC;color:#8A4B0A}
.status-off{background:#ECEDE8;color:#3C4551}
.dot{width:8px;height:8px;border-radius:50%;background:currentColor;flex:none}
.dot-pulse{animation:pulse 1.4s ease-in-out infinite}
@keyframes pulse{0%,100%{opacity:1}50%{opacity:.35}}
@media (prefers-reduced-motion: reduce){.dot-pulse{animation:none}}
.tip{display:inline-flex;color:#8A919C;cursor:help}
.field{display:flex;flex-direction:column;gap:6px}
.kbd{font-family:"IBM Plex Mono",ui-monospace,Menlo,monospace;font-size:11px;padding:1px 6px;border:1px solid #CFD4DA;border-bottom-width:2px;border-radius:5px;background:#FFFFFF;color:#3C4551}
.h1{font-size:22px;font-weight:600;letter-spacing:-.01em;margin:0}
.h2{font-size:16px;font-weight:600;margin:0}
.muted{color:#5B6470}
.caret{display:inline-block;width:2px;height:16px;background:#0F766E;vertical-align:-3px;margin-left:2px;animation:pulse 1s steps(2) infinite}
.md h3{font-size:14px;margin:14px 0 6px;font-weight:600}
.md p,.md li{font-size:13px;line-height:1.6;margin:0 0 4px}
.md ul{padding-left:18px;margin:0}
.sw{width:40px;height:40px;border-radius:8px;border:1px solid rgba(0,0,0,.08)}
"""

def page(title, body, w, h, lang="vi", props=None):
    props = props or {}
    props["$preview"] = {"width": w, "height": h}
    pj = json.dumps(props, ensure_ascii=False).replace("&", "&amp;").replace("'", "&#39;")
    return f"""<!doctype html>
<html lang="{lang}">
<head>
<meta charset="utf-8">
<title>{title}</title>
<script src="./support.js"></script>
</head>
<body>
<x-dc>
<helmet>
{FONT_LINK}
<style>{CSS}</style>
</helmet>
{body}
</x-dc>
<script type="text/x-dc" data-dc-script data-props='{pj}'>
class Component extends DCLogic {{
renderVals() {{
return {{}};
}}
}}
</script>
</body>
</html>
"""

# ---------------------------------------------------------------- shell
def sidebar(active, show_ad=True, job=True, w=260):
    def item(name, icon, label, href):
        cls = "nav-item nav-item-active" if active == name else "nav-item"
        cur = ' aria-current="page"' if active == name else ""
        return f'<a class="{cls}" href="{href}"{cur}>{ic(icon)}<span>{label}</span></a>'
    jobcard = ""
    if job:
        jobcard = f"""
<div style="display: flex; flex-direction: column; gap: 8px; padding: 12px; border-radius: 10px; background: #FFFFFF; border: 1px solid #E1E4E8;">
  <div style="display: flex; align-items: center; gap: 8px; font-size: 12px; font-weight: 600; color: #8A4B0A;">{ic("activity", 14)}<span>1 job đang chạy</span></div>
  <div style="font-size: 13px; font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">zoom-recording-0912.mp4</div>
  <div style="height: 6px; border-radius: 3px; background: #ECEDE8; overflow: hidden;"><div style="width: 36%; height: 100%; background: #0F766E;"></div></div>
  <div style="display: flex; justify-content: space-between; font-size: 12px; color: #5B6470;"><span class="mono">32 / 90 phút</span><a href="Transcript.dc.html" style="font-weight: 600; text-decoration: none;">Mở</a></div>
</div>"""
    ad = ""
    if show_ad:
        ad = f"""
<div style="display: flex; flex-direction: column; gap: 6px;">
  <div style="display: flex; align-items: center; justify-content: space-between; font-size: 11px; color: #5B6470;">
    <span style="font-weight: 600; letter-spacing: .04em; text-transform: uppercase;">Sponsored</span>
    <button class="tip" type="button" aria-label="Vì sao tôi thấy quảng cáo này" style="border: 0; background: transparent; padding: 0;">{ic("help", 14)}</button>
  </div>
  <a href="https://relipasoft.com" target="_blank" rel="noopener" style="display: flex; flex-direction: column; gap: 0; border-radius: 10px; overflow: hidden; border: 1px solid #E1E4E8; background: #FFFFFF; text-decoration: none; color: #171A1F;">
    <div style="height: 64px; background: #E6F3F1; display: flex; align-items: center; justify-content: center; color: #0B5D57;">{ic("image", 22, "#0B5D57")}</div>
    <div style="padding: 8px 10px; display: flex; flex-direction: column; gap: 2px;">
      <div style="font-size: 12px; font-weight: 600; line-height: 1.3;">Relipa — đối tác offshore Nhật–Việt</div>
      <div style="font-size: 11px; color: #5B6470;">relipasoft.com</div>
    </div>
  </a>
  <a href="mailto:ads@transkun.app?subject=Report%20creative%20house-relipa-01" style="font-size: 11px; color: #5B6470; text-decoration: none; display: inline-flex; align-items: center; gap: 4px;">{ic("flag", 12)}<span>Báo cáo quảng cáo</span></a>
</div>"""
    return f"""
<aside style="width: {w}px; flex: none; display: flex; flex-direction: column; gap: 6px; padding: 16px 12px; background: #F0F0EB; border-right: 1px solid #E1E4E8; height: 100%;">
  <div style="display: flex; align-items: center; gap: 10px; padding: 4px 8px 14px;">
    <div style="width: 30px; height: 30px; border-radius: 8px; background: #0F766E; display: flex; align-items: center; justify-content: center;">{ic("wave", 18, "#FFFFFF", 2.2)}</div>
    <div style="font-size: 16px; font-weight: 700; letter-spacing: -.01em;">trans-kun</div>
  </div>
  {item("home", "home", "Trang chủ", "Home.dc.html")}
  {item("live", "radio", "Live", "LiveSetup.dc.html")}
  {item("settings", "settings", "Cài đặt", "Settings.dc.html")}
  <div style="flex-grow: 1;"></div>
  {jobcard}
  {ad}
</aside>"""

def shell(active, main, w=1280, h=800, show_ad=True, job=True):
    return f"""<div style="width: {w}px; height: {h}px; display: flex; flex-direction: row; background: #F6F6F2; overflow: hidden;">
{sidebar(active, show_ad, job)}
<main style="flex-grow: 1; min-width: 0; height: 100%; display: flex; flex-direction: column; overflow: hidden;">
{main}
</main>
</div>"""

# ---------------------------------------------------------------- onboarding
def stepper(step):
    names = ["Ngôn ngữ", "Dữ liệu", "API key"]
    out = []
    for i, n in enumerate(names, 1):
        if i < step:
            circ = f'<div style="width: 24px; height: 24px; border-radius: 50%; background: #0F766E; color: #FFFFFF; display: flex; align-items: center; justify-content: center;">{ic("check", 14, "#FFFFFF", 2.5)}</div>'
            col = "#0B5D57"
        elif i == step:
            circ = f'<div style="width: 24px; height: 24px; border-radius: 50%; background: #0F766E; color: #FFFFFF; display: flex; align-items: center; justify-content: center; font-size: 12px; font-weight: 700;">{i}</div>'
            col = "#171A1F"
        else:
            circ = f'<div style="width: 24px; height: 24px; border-radius: 50%; border: 1.5px solid #CFD4DA; color: #5B6470; display: flex; align-items: center; justify-content: center; font-size: 12px; font-weight: 600;">{i}</div>'
            col = "#5B6470"
        out.append(f'<div style="display: flex; align-items: center; gap: 8px; font-size: 13px; font-weight: 600; color: {col};">{circ}<span>{n}</span></div>')
        if i < 3:
            out.append('<div style="width: 40px; height: 1px; background: #CFD4DA;"></div>')
    return '<div style="display: flex; align-items: center; gap: 12px;">' + "".join(out) + "</div>"

def onboarding(step, title, sub, content, footer):
    return f"""<div style="width: 1280px; height: 800px; display: flex; align-items: center; justify-content: center; background: #F6F6F2;">
<div style="width: 600px; display: flex; flex-direction: column; gap: 28px;">
  <div style="display: flex; align-items: center; justify-content: space-between;">
    <div style="display: flex; align-items: center; gap: 10px;">
      <div style="width: 30px; height: 30px; border-radius: 8px; background: #0F766E; display: flex; align-items: center; justify-content: center;">{ic("wave", 18, "#FFFFFF", 2.2)}</div>
      <div style="font-size: 16px; font-weight: 700;">trans-kun</div>
    </div>
    {stepper(step)}
  </div>
  <div class="card" style="padding: 32px; display: flex; flex-direction: column; gap: 24px;">
    <div style="display: flex; flex-direction: column; gap: 6px;">
      <h1 class="h1">{title}</h1>
      <p class="muted" style="margin: 0; font-size: 14px;">{sub}</p>
    </div>
    {content}
    <div style="display: flex; align-items: center; justify-content: space-between; padding-top: 8px; border-top: 1px solid #EDEFF1;">
      {footer}
    </div>
  </div>
</div>
</div>"""

def lang_option(code, native, hint, checked=False, sys=False):
    border = "#0F766E" if checked else "#E1E4E8"
    bg = "#F3FAF9" if checked else "#FFFFFF"
    chk = ' checked="checked"' if checked else ""
    tag = '<span class="badge badge-memo">Theo hệ thống</span>' if sys else ""
    return f"""<label style="display: flex; align-items: center; gap: 14px; padding: 14px 16px; border: 1.5px solid {border}; border-radius: 10px; background: {bg}; cursor: pointer;">
  <input type="radio" name="ui-lang" value="{code}"{chk} style="width: 18px; height: 18px; accent-color: #0F766E; margin: 0;">
  <div style="display: flex; flex-direction: column; gap: 2px; flex-grow: 1;">
    <div style="font-size: 15px; font-weight: 600; display: flex; align-items: center; gap: 8px;"><span>{native}</span>{tag}</div>
    <div class="help">{hint}</div>
  </div>
</label>"""

ob_lang = onboarding(1, "Chọn ngôn ngữ giao diện", "Bạn có thể đổi lại bất cứ lúc nào trong Cài đặt.",
  '<div style="display: flex; flex-direction: column; gap: 10px;">'
  + lang_option("vi", "Tiếng Việt", "Memo mẫu 議事録 tiếng Việt, nhãn thời gian theo múi giờ máy", True, True)
  + lang_option("en", "English", "Default memo templates in English")
  + lang_option("ja", "日本語", "議事録テンプレート（日本語）を既定で使用")
  + '</div>',
  '<span class="help">Bước 1 / 3</span><a class="btn btn-primary btn-lg" href="OnboardingConsent.dc.html">Tiếp tục</a>')

ob_consent = onboarding(2, "Dữ liệu buổi họp của bạn đi đâu?",
  "Chúng tôi cần bạn đồng ý trước khi bất kỳ tính năng nào gửi dữ liệu.",
  f"""<div style="display: flex; align-items: center; gap: 0; padding: 20px; border-radius: 10px; background: #F6F6F2;">
  <div style="flex: 1; display: flex; flex-direction: column; align-items: center; gap: 8px; text-align: center;">
    <div style="width: 48px; height: 48px; border-radius: 12px; background: #FFFFFF; border: 1px solid #E1E4E8; display: flex; align-items: center; justify-content: center; color: #171A1F;">{ic("laptop", 22)}</div>
    <div style="font-size: 13px; font-weight: 600;">Máy của bạn</div>
    <div class="help">Audio, transcript, ghi chú<br>lưu trong app</div>
  </div>
  <div style="display: flex; flex-direction: column; align-items: center; gap: 4px; width: 140px; color: #0F766E;">
    {ic("arrow-right", 22, "#0F766E")}
    <div style="font-size: 11px; font-weight: 600; text-align: center;">Bằng API key<br>của bạn</div>
  </div>
  <div style="flex: 1; display: flex; flex-direction: column; align-items: center; gap: 8px; text-align: center;">
    <div style="width: 48px; height: 48px; border-radius: 12px; background: #FFFFFF; border: 1px solid #E1E4E8; display: flex; align-items: center; justify-content: center; color: #171A1F;">{ic("cloud", 22)}</div>
    <div style="font-size: 13px; font-weight: 600;">Google Gemini</div>
    <div class="help">Transcribe, dịch,<br>sinh memo</div>
  </div>
</div>
<ul style="margin: 0; padding-left: 0; list-style: none; display: flex; flex-direction: column; gap: 10px;">
  <li style="display: flex; gap: 10px; font-size: 14px; line-height: 1.5;">{ic("check", 18, "#0F766E", 2.2)}<span><strong>Audio và transcript</strong> của buổi họp được gửi tới Google Gemini bằng API key do bạn cung cấp.</span></li>
  <li style="display: flex; gap: 10px; font-size: 14px; line-height: 1.5;">{ic("check", 18, "#0F766E", 2.2)}<span><strong>Không có server trung gian</strong> của trans-kun. Chúng tôi không thấy, không lưu nội dung họp.</span></li>
  <li style="display: flex; gap: 10px; font-size: 14px; line-height: 1.5;">{ic("check", 18, "#0F766E", 2.2)}<span><strong>Không telemetry</strong> mặc định. Nhật ký chẩn đoán không chứa nội dung.</span></li>
</ul>
<div class="help">Xem <a href="https://transkun.app/privacy" target="_blank" rel="noopener">Chính sách quyền riêng tư</a> (mở trong trình duyệt). Văn bản đồng ý phiên bản 1 · 2026-09.</div>""",
  '<a class="btn btn-ghost btn-lg" href="OnboardingLang.dc.html">Không đồng ý</a><a class="btn btn-primary btn-lg" href="OnboardingKey.dc.html">Đồng ý và tiếp tục</a>')

ob_key = onboarding(3, "Dán Gemini API key", "Bạn tự mang key (BYOK). Key được lưu trong Keychain của hệ điều hành, không nằm trong file cấu hình.",
  f"""<div class="field">
  <label class="label" for="ob-key">Gemini API key <button class="tip" type="button" aria-label="Trợ giúp" style="border: 0; background: transparent; padding: 0;">{ic("help", 14)}</button></label>
  <div style="display: flex; gap: 8px;">
    <div style="position: relative; flex-grow: 1; display: flex;">
      <input id="ob-key" class="input mono" type="password" value="AIzaSyD4k1xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx" style="padding-right: 40px;">
      <button class="btn btn-ghost btn-icon btn-sm" type="button" aria-label="Hiện key" style="position: absolute; right: 3px; top: 3px;">{ic("eye", 16)}</button>
    </div>
    <button class="btn btn-secondary" type="button">{ic("key", 16)}<span>Kiểm tra key</span></button>
  </div>
  <div class="help">Chấp nhận cả định dạng <span class="mono">AIza…</span> và <span class="mono">AQ.…</span>. Nhiều key cách nhau dấu phẩy để xoay vòng khi hết quota.</div>
</div>
<div role="status" style="display: flex; align-items: flex-start; gap: 10px; padding: 12px 14px; border-radius: 10px; background: #E0F2F1; color: #0B5D57;">
  {ic("check", 18, "#0B5D57", 2.2)}
  <div style="display: flex; flex-direction: column; gap: 2px;">
    <div style="font-size: 14px; font-weight: 600;">Key hợp lệ · 14 model khả dụng</div>
    <div style="font-size: 12px;">Mặc định: <span class="mono">gemini-flash-lite-latest</span> (file, memo) · <span class="mono">gemini-3.5-live-translate-preview</span> (live). Đổi trong Cài đặt.</div>
  </div>
</div>""",
  '<a class="btn btn-ghost btn-lg" href="HomeEmpty.dc.html">Bỏ qua, nhập sau</a><a class="btn btn-primary btn-lg" href="HomeEmpty.dc.html">Vào ứng dụng</a>')

# ---------------------------------------------------------------- home
def header(title, right=""):
    return f"""<div style="display: flex; align-items: center; justify-content: space-between; gap: 16px; height: 64px; padding: 0 24px; border-bottom: 1px solid #E1E4E8; background: #FFFFFF; flex: none;">
  <h1 class="h1">{title}</h1>
  <div style="display: flex; align-items: center; gap: 8px;">{right}</div>
</div>"""

home_empty_main = header("Trang chủ", f'<a class="btn btn-secondary" href="Settings.dc.html">{ic("settings",16)}<span>Cài đặt</span></a>') + f"""
<div style="flex-grow: 1; display: flex; align-items: center; justify-content: center; padding: 32px;">
  <div style="display: flex; flex-direction: column; gap: 20px; width: 760px;">
    <div style="display: flex; flex-direction: column; gap: 4px; text-align: center;">
      <div style="font-size: 20px; font-weight: 600;">Chưa có phiên nào</div>
      <div class="muted">Bắt đầu bằng một trong hai cách bên dưới. Phiên sẽ xuất hiện ở đây.</div>
    </div>
    <div style="display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px;">
      <div style="display: flex; flex-direction: column; align-items: center; gap: 14px; padding: 36px 24px; border: 2px dashed #B7DDD8; border-radius: 14px; background: #FFFFFF; text-align: center;">
        <div style="width: 56px; height: 56px; border-radius: 14px; background: #E6F3F1; display: flex; align-items: center; justify-content: center; color: #0B5D57;">{ic("upload", 26, "#0B5D57")}</div>
        <div style="display: flex; flex-direction: column; gap: 4px;">
          <div style="font-size: 16px; font-weight: 600;">Kéo file audio / video vào đây</div>
          <div class="help">mp3 · m4a · wav · flac · ogg · aiff · caf · mp4 · mov · mkv · webm</div>
        </div>
        <a class="btn btn-secondary" href="Transcript.dc.html">{ic("file",16)}<span>Chọn file…</span></a>
      </div>
      <div style="display: flex; flex-direction: column; align-items: center; gap: 14px; padding: 36px 24px; border: 1px solid #E1E4E8; border-radius: 14px; background: #FFFFFF; text-align: center;">
        <div style="width: 56px; height: 56px; border-radius: 14px; background: #FDE8E8; display: flex; align-items: center; justify-content: center;">{ic("radio", 26, "#A11E1E")}</div>
        <div style="display: flex; flex-direction: column; gap: 4px;">
          <div style="font-size: 16px; font-weight: 600;">Transcribe buổi họp đang diễn ra</div>
          <div class="help">Zoom, Teams, Google Meet… kèm bản dịch realtime và ghi âm bền</div>
        </div>
        <a class="btn btn-primary" href="LiveSetup.dc.html">{ic("radio",16,"#FFFFFF")}<span>Bắt đầu Live</span></a>
      </div>
    </div>
    <div style="display: flex; align-items: center; gap: 10px; padding: 12px 14px; border-radius: 10px; background: #FFF7E6; border: 1px solid #F5D9A8; color: #8A4B0A; font-size: 13px;">
      {ic("key", 16, "#8A4B0A")}<span style="flex-grow: 1;">Chưa có API key hợp lệ — các tính năng cần Gemini đang tắt. Bạn vẫn xem và phát lại phiên cũ được.</span><a href="Settings.dc.html" style="font-weight: 600; color: #8A4B0A;">Nhập key</a>
    </div>
  </div>
</div>"""

def session_row(name, date, kind, dur, tags, flag="", href="Transcript.dc.html"):
    kb = '<span class="badge badge-live">LIVE</span>' if kind == "live" else '<span class="badge badge-file">FILE</span>'
    chips = "".join(f'<span class="chip chip-sm">{t}</span>' for t in tags) if tags else '<span class="help">—</span>'
    return f"""<a class="row" href="{href}">
  <div style="display: flex; align-items: center; gap: 10px; min-width: 0;">{ic("radio" if kind=="live" else "file", 16, "#5B6470")}<span style="font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{name}</span>{flag}</div>
  <div class="mono" style="font-size: 13px; color: #3C4551;">{date}</div>
  <div>{kb}</div>
  <div class="mono" style="font-size: 13px; text-align: right;">{dur}</div>
  <div style="display: flex; gap: 6px; overflow: hidden;">{chips}</div>
  <button class="btn btn-ghost btn-icon btn-sm" type="button" aria-label="Thao tác khác">{ic("more",16)}</button>
</a>"""

B_MEMO = '<span class="badge badge-memo">Memo</span>'
B_AUDIO = '<span class="badge badge-audio">Audio</span>'
B_PART = '<span class="badge badge-partial">Thiếu 1 khoảng</span>'
B_REC = '<span class="badge badge-recover">Phục hồi</span>'

home_main = header("Trang chủ", f"""
<div style="position: relative; width: 300px; display: flex;">
  <span style="position: absolute; left: 10px; top: 9px; color: #5B6470;">{ic("search",16)}</span>
  <input class="input" type="search" placeholder="Tìm theo tên phiên" value="admin" aria-label="Tìm theo tên phiên" style="padding-left: 34px; padding-right: 32px;">
  <button class="btn btn-ghost btn-icon btn-sm" type="button" aria-label="Xoá tìm kiếm" style="position: absolute; right: 3px; top: 3px;">{ic("x",14)}</button>
</div>
<a class="btn btn-secondary" href="Transcript.dc.html">{ic("file",16)}<span>Chọn file</span></a>
<a class="btn btn-primary" href="LiveSetup.dc.html">{ic("radio",16,"#FFFFFF")}<span>Live</span></a>""") + f"""
<div style="flex-grow: 1; display: flex; flex-direction: column; gap: 16px; padding: 16px 24px 20px; overflow: hidden;">
  <div style="display: flex; align-items: center; gap: 8px; flex-wrap: wrap;">
    <span class="label" style="margin-right: 4px;">{ic("tag",14)}Tag</span>
    <button class="chip chip-on" type="button" aria-pressed="true">KH-ABC {ic("x",12)}</button>
    <button class="chip chip-on" type="button" aria-pressed="true">sprint-12 {ic("x",12)}</button>
    <button class="chip" type="button" aria-pressed="false">KH-XYZ</button>
    <button class="chip" type="button" aria-pressed="false">sales</button>
    <button class="chip" type="button" aria-pressed="false">nội bộ</button>
    <button class="chip" type="button" aria-expanded="false" aria-haspopup="dialog" style="border-style: dashed; border-color: #CFD4DA; background: transparent;">+ 9 tag khác {ic("chevron-down",12)}</button>
    <span style="width: 1px; height: 18px; background: #E1E4E8;"></span>
    <button class="chip" type="button" aria-pressed="false">Chưa gắn tag</button>
    <span style="flex-grow: 1;"></span>
    <button class="btn btn-ghost btn-sm" type="button">Quản lý tag</button>
  </div>
  <div style="display: flex; align-items: center; gap: 12px; padding: 10px 14px; border: 1.5px dashed #B7DDD8; border-radius: 10px; color: #0B5D57; background: #F3FAF9; font-size: 13px;">
    {ic("upload",16,"#0B5D57")}<span>Kéo file vào bất kỳ đâu để transcribe · mỗi file một phiên, xử lý tuần tự</span>
  </div>
  <div class="card" style="display: flex; align-items: center; gap: 16px; padding: 12px 16px; border-color: #F5D9A8; background: #FFFBF2;">
    <div style="width: 36px; height: 36px; border-radius: 10px; background: #FDF0DC; display: flex; align-items: center; justify-content: center;">{ic("activity",18,"#8A4B0A")}</div>
    <div style="display: flex; flex-direction: column; gap: 6px; flex-grow: 1; min-width: 0;">
      <div style="display: flex; align-items: center; justify-content: space-between; gap: 12px;">
        <div style="font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">Đang transcribe · zoom-recording-0912.mp4</div>
        <div class="mono" style="font-size: 13px; color: #3C4551;">32 / 90 phút · 36%</div>
      </div>
      <div style="height: 6px; border-radius: 3px; background: #ECEDE8; overflow: hidden;"><div style="width: 36%; height: 100%; background: #0F766E;"></div></div>
      <div class="help">Chunk 7/18 · key 2/2 đang dùng · 1 lần thử lại (429)</div>
    </div>
    <a class="btn btn-secondary btn-sm" href="Transcript.dc.html">Mở</a>
    <button class="btn btn-danger-soft btn-sm" type="button">Huỷ</button>
  </div>
  <div class="card" style="flex-grow: 1; display: flex; flex-direction: column; overflow: hidden;">
    <div class="th"><div>Tên</div><div>Ngày</div><div>Loại</div><div style="text-align: right;">Thời lượng</div><div>Tag</div><div></div></div>
    {session_row("Họp KH-ABC — review admin UI", "18/09 14:00", "live", "1:02:10", ["KH-ABC","sprint-12"])}
    {session_row("Sprint-12 planning (recording)", "17/09 09:30", "file", "1:29:58", ["KH-ABC","sprint-12","nội bộ"], B_PART)}
    {session_row("2026-09-16 10:05", "16/09 10:05", "live", "0:22:41", ["KH-ABC","sprint-12"], B_REC)}
    {session_row("Demo admin dashboard cho KH-ABC", "12/09 15:00", "file", "0:41:07", ["KH-ABC","sprint-12","demo"])}
    {session_row("Kickoff KH-ABC — sprint 12", "10/09 13:30", "live", "1:15:22", ["KH-ABC","sprint-12","kickoff"])}
    {session_row("Test", "09/09 11:12", "live", "0:01:33", ["KH-ABC","sprint-12"])}
    <div style="flex-grow: 1;"></div>
    <div style="display: flex; align-items: center; justify-content: space-between; padding: 10px 12px; border-top: 1px solid #EDEFF1; font-size: 12px; color: #5B6470;">
      <span>6 / 38 phiên · lọc: “admin” + 2 tag</span>
      <button class="btn btn-ghost btn-sm" type="button">Xoá bộ lọc</button>
    </div>
  </div>
</div>"""

# ---------------------------------------------------------------- transcript detail
def seg(ts, text, speaker=None, active=False, mark=None):
    sp = f'<span class="speaker">{speaker}</span>' if speaker else ""
    if mark:
        text = text.replace(mark, f'<span class="mark">{mark}</span>')
    cls = "seg seg-active" if active else "seg"
    return f'<a class="{cls}" href="#t{ts.replace(":","")}" style="text-decoration: none;"><span class="ts">{ts}</span><span class="seg-text">{sp}{text}</span></a>'

JA = [
 ("00:41:52", "管理画面のユーザー一覧ですが、CSVエクスポートは初期リリースに含めますか。", "田中"),
 ("00:42:03", "はい、含めます。ただしフィルタ条件を保持したままエクスポートしたいです。", "田中"),
 ("00:42:15", "承知しました。フィルタはサーバー側で適用してからCSVを生成します。", "Linh"),
 ("00:42:31", "権限のところ、閲覧のみのロールでもエクスポートできてしまうと困ります。", "田中"),
 ("00:42:44", "エクスポートは編集権限以上に限定します。仕様書に追記します。", "Linh"),
]
transcript_left = "".join([
  seg("00:41:52", "Về danh sách người dùng ở màn admin, xuất CSV có nằm trong bản phát hành đầu không?", "田中"),
  seg("00:42:03", "Có. Nhưng tôi muốn giữ nguyên điều kiện lọc khi xuất.", "田中"),
  seg("00:42:15", "Đã rõ. Bộ lọc sẽ được áp dụng phía server rồi mới sinh CSV.", "Linh", True),
  seg("00:42:31", "Về phân quyền, role chỉ xem mà vẫn xuất được thì không ổn.", "田中"),
  seg("00:42:44", "Xuất chỉ dành cho quyền chỉnh sửa trở lên. Tôi sẽ bổ sung vào spec.", "Linh"),
])

def missing_divider(rng):
    return f"""<div style="display: flex; align-items: center; gap: 10px; padding: 8px 12px; margin: 4px 0; border-radius: 8px; background: #FDF0DC; color: #8A4B0A; font-size: 12px; font-weight: 600;">
  {ic("alert",14,"#8A4B0A")}<span>Thiếu {rng}</span><span style="flex-grow: 1;"></span><button class="btn btn-secondary btn-sm" type="button" style="height: 26px;">Chạy lại khoảng này</button>
</div>"""

transcript_main = f"""
<div style="display: flex; align-items: center; gap: 12px; height: 64px; padding: 0 20px; border-bottom: 1px solid #E1E4E8; background: #FFFFFF; flex: none;">
  <a class="btn btn-ghost btn-icon" href="Home.dc.html" aria-label="Về trang chủ">{ic("chevron-left",18)}</a>
  <div style="display: flex; flex-direction: column; gap: 2px; min-width: 0; flex-grow: 1;">
    <div style="display: flex; align-items: center; gap: 8px; min-width: 0;">
      <h1 class="h1" style="font-size: 18px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">Họp KH-ABC — review admin UI</h1>
      <button class="btn btn-ghost btn-icon btn-sm" type="button" aria-label="Đổi tên">{ic("edit",14)}</button>
    </div>
    <div style="display: flex; align-items: center; gap: 6px; font-size: 12px; color: #5B6470;">
      <span class="badge badge-live">LIVE</span><span class="mono">18/09/2026 14:00 · 1:02:10 · 412 segment</span>
      <span>·</span><button class="chip" type="button" style="height: 22px;">KH-ABC</button><button class="chip" type="button" style="height: 22px;">sprint-12</button><button class="chip" type="button" style="height: 22px; border-style: dashed; border-color: #CFD4DA; background: transparent;">+ Tag</button>
    </div>
  </div>
  <div style="position: relative; width: 240px; display: flex;">
    <span style="position: absolute; left: 10px; top: 9px; color: #5B6470;">{ic("search",16)}</span>
    <input class="input" type="search" value="CSV" aria-label="Tìm trong transcript" style="padding-left: 34px; padding-right: 96px;">
    <div style="position: absolute; right: 3px; top: 3px; display: flex; align-items: center; gap: 2px;">
      <span class="mono" style="font-size: 12px; color: #5B6470; padding: 0 4px;">2/3</span>
      <button class="btn btn-ghost btn-icon btn-sm" type="button" aria-label="Kết quả trước">{ic("chevron-up",14)}</button>
      <button class="btn btn-ghost btn-icon btn-sm" type="button" aria-label="Kết quả sau">{ic("chevron-down",14)}</button>
    </div>
  </div>
  <button class="btn btn-secondary" type="button">{ic("download",16)}<span>Export</span>{ic("chevron-down",14)}</button>
  <button class="btn btn-secondary btn-icon" type="button" aria-label="Copy toàn bộ transcript">{ic("copy",16)}</button>
  <button class="btn btn-ghost btn-icon" type="button" aria-label="Thao tác khác">{ic("more",16)}</button>
</div>

<div style="display: flex; align-items: center; gap: 14px; height: 56px; padding: 0 20px; background: #FFFFFF; border-bottom: 1px solid #E1E4E8; flex: none;">
  <button class="btn btn-primary btn-icon" type="button" aria-label="Tạm dừng" style="border-radius: 50%; width: 40px; height: 40px;">{ic("pause",18,"#FFFFFF")}</button>
  <span class="mono" style="font-size: 13px; color: #3C4551; width: 132px;">00:42:15 / 01:02:10</span>
  <div style="flex-grow: 1; position: relative; height: 24px; display: flex; align-items: center; cursor: pointer;" role="slider" aria-label="Vị trí phát" aria-valuemin="0" aria-valuemax="3730" aria-valuenow="2535" tabindex="0">
    <div style="width: 100%; height: 4px; border-radius: 2px; background: #E1E4E8; overflow: hidden;"><div style="width: 68%; height: 100%; background: #0F766E;"></div></div>
    <div style="position: absolute; left: 68%; top: 5px; width: 14px; height: 14px; margin-left: -7px; border-radius: 50%; background: #FFFFFF; border: 2px solid #0F766E;"></div>
  </div>
  <button class="btn btn-ghost btn-sm mono" type="button">1.0×</button>
  <button class="btn btn-ghost btn-icon btn-sm" type="button" aria-label="Âm lượng">{ic("speaker",16)}</button>
</div>

<div style="display: flex; align-items: center; gap: 12px; padding: 10px 20px; background: #FDF0DC; border-bottom: 1px solid #F5D9A8; color: #8A4B0A; font-size: 13px; flex: none;">
  {ic("alert",16,"#8A4B0A")}
  <span style="flex-grow: 1;"><strong>Bản Transcribe lại chưa hoàn chỉnh</strong> — thiếu 1 khoảng: <span class="mono">45:00–50:00</span>. Bản live vẫn được giữ nguyên.</span>
  <button class="btn btn-secondary btn-sm" type="button">Chạy lại phần thiếu</button>
  <button class="btn btn-ghost btn-sm" type="button">Chạy lại toàn bộ</button>
</div>

<div style="flex-grow: 1; display: flex; min-height: 0;">
  <section style="flex-grow: 1; min-width: 0; display: flex; flex-direction: column; border-right: 1px solid #E1E4E8; background: #FFFFFF;">
    <div style="display: flex; align-items: center; gap: 12px; height: 44px; padding: 0 12px 0 20px; border-bottom: 1px solid #EDEFF1; flex: none;">
      <div class="segmented" role="group" aria-label="Chọn bản transcript">
        <button class="seg-btn" type="button">Bản live</button>
        <button class="seg-btn" type="button">Transcribe lại</button>
        <button class="seg-btn seg-btn-on" type="button" aria-pressed="true">Cạnh nhau</button>
      </div>
      <span class="help">Click segment ở cột nào cũng seek trình phát · export lấy bản đang chọn</span>
      <span style="flex-grow: 1;"></span>
      <span class="help mono">offset +0.0 s</span>
    </div>
    <div style="display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); flex-grow: 1; min-height: 0;">
      <div style="display: flex; flex-direction: column; gap: 2px; padding: 8px; border-right: 1px solid #EDEFF1; overflow: hidden;">
        <div style="display: flex; align-items: center; gap: 8px; padding: 4px 12px 8px; font-size: 12px; font-weight: 600; color: #5B6470; text-transform: uppercase; letter-spacing: .04em;">{ic("radio",14)}<span>Bản live</span></div>
        {seg("00:41:52", "管理画面のユーザー一覧ですが、CSVエクスポートは初期リリースに含めますか。", None, False, "CSV")}
        {seg("00:42:03", "はい、含めます。ただしフィルタ条件を保持したままエクスポートしたいです。")}
        {seg("00:42:15", "承知しました。フィルタはサーバー側で適用してからCSVを生成します。", None, True, "CSV")}
        {seg("00:42:31", "権限のところ、閲覧のみのロールでもエクスポートできてしまうと困ります。")}
        {seg("00:42:44", "エクスポートは編集権限以上に限定します。仕様書に追記します。")}
        <div style="display: flex; align-items: center; gap: 10px; padding: 8px 12px; margin: 4px 0; border-radius: 8px; background: #ECEDE8; color: #3C4551; font-size: 12px; font-weight: 600;">{ic("wifi-off",14)}<span>Mất kết nối 45:02–48:10 (đã ghi âm, xem bản Transcribe lại)</span></div>
      </div>
      <div style="display: flex; flex-direction: column; gap: 2px; padding: 8px; overflow: hidden;">
        <div style="display: flex; align-items: center; gap: 8px; padding: 4px 12px 8px; font-size: 12px; font-weight: 600; color: #5B6470; text-transform: uppercase; letter-spacing: .04em;">{ic("memo",14)}<span>Transcribe lại</span><span class="badge badge-partial">Partial</span></div>
        {seg("00:41:52", "管理画面のユーザー一覧ですが、CSVエクスポートは初期リリースに含めますか。", None, False, "CSV")}
        {seg("00:42:03", "はい、含めます。ただしフィルタ条件を保持したままエクスポートしたいです。")}
        {seg("00:42:15", "承知しました。フィルタはサーバー側で適用してからCSVを生成します。", None, True, "CSV")}
        {seg("00:42:31", "権限のところ、閲覧のみのロールでもエクスポートできてしまうと困ります。")}
        {seg("00:42:44", "エクスポートは編集権限以上に限定します。仕様書に追記します。")}
        {missing_divider("45:00–50:00")}
      </div>
    </div>
  </section>

  <aside style="width: 360px; flex: none; display: flex; flex-direction: column; background: #FAFAF7;">
    <div style="display: flex; align-items: center; gap: 2px; height: 44px; padding: 6px 12px; border-bottom: 1px solid #E1E4E8; flex: none;">
      <div class="segmented" role="tablist" aria-label="Panel phụ">
        <button class="seg-btn seg-btn-on" role="tab" type="button" aria-selected="true">Memo</button>
        <button class="seg-btn" role="tab" type="button" aria-selected="false">Ghi chú</button>
      </div>
    </div>
    <div style="display: flex; flex-direction: column; gap: 10px; padding: 12px; border-bottom: 1px solid #E1E4E8; flex: none;">
      <div style="display: flex; gap: 8px;">
        <select class="select" aria-label="Template memo" style="flex-grow: 1;"><option>議事録 (mặc định)</option><option>Biên bản họp sales</option><option>Action items</option></select>
        <button class="btn btn-primary" type="button">{ic("memo",16,"#FFFFFF")}<span>Sinh lại</span></button>
      </div>
      <div style="display: flex; align-items: center; gap: 8px;">
        <span class="badge badge-token">{ic("zap",11,"#8A4B0A")} Tốn token Gemini</span>
        <span class="help">Sinh từ bản live + ghi chú · 14:05</span>
      </div>
    </div>
    <div class="md" style="flex-grow: 1; padding: 14px 16px; overflow: hidden; font-size: 13px;">
      <h3 style="margin-top: 0;">議事録 — Review admin UI (18/09)</h3>
      <p><strong>Tham dự:</strong> 田中さん (KH-ABC), Linh (Relipa)</p>
      <h3>Quyết định</h3>
      <ul>
        <li>Xuất CSV nằm trong bản phát hành đầu, giữ nguyên điều kiện lọc.</li>
        <li>Chỉ role chỉnh sửa trở lên mới được xuất.</li>
      </ul>
      <h3>Action items</h3>
      <ul>
        <li>Linh — bổ sung điều kiện phân quyền export vào spec (hạn 20/09).</li>
        <li>田中 — xác nhận danh sách cột CSV.</li>
      </ul>
      <h3>Ghi chú của bạn</h3>
      <p class="muted">“Hỏi lại về giới hạn 10k dòng khi export.”</p>
    </div>
    <div style="display: flex; gap: 8px; padding: 12px; border-top: 1px solid #E1E4E8; flex: none;">
      <button class="btn btn-secondary btn-sm" type="button">{ic("copy",14)}<span>Copy</span></button>
      <button class="btn btn-secondary btn-sm" type="button">{ic("download",14)}<span>Tải .md</span></button>
      <span style="flex-grow: 1;"></span>
      <button class="btn btn-ghost btn-sm" type="button">{ic("refresh",14)}<span>Transcribe lại</span></button>
    </div>
  </aside>
</div>"""

# ---------------------------------------------------------------- live setup
def source_option(icon_names, title, desc, checked=False, badge=None, mic=False):
    border = "#0F766E" if checked else "#E1E4E8"
    bg = "#F3FAF9" if checked else "#FFFFFF"
    chk = ' checked="checked"' if checked else ""
    icons = "".join(ic(n, 18, "#0B5D57" if checked else "#3C4551") for n in icon_names)
    bd = f'<span class="badge badge-memo">{badge}</span>' if badge else ""
    microw = ""
    if mic:
        microw = f"""<div style="display: flex; align-items: center; gap: 8px; margin-top: 4px;">
      <label class="help" for="mic-sel" style="white-space: nowrap;">Microphone</label>
      <select id="mic-sel" class="select" style="height: 32px; font-size: 13px;"><option>AirPods Pro (mặc định)</option><option>MacBook Pro Microphone</option></select>
      <button class="btn btn-ghost btn-icon btn-sm" type="button" aria-label="Làm mới danh sách mic">{ic("refresh",14)}</button>
    </div>"""
    return f"""<label style="display: flex; align-items: flex-start; gap: 14px; padding: 14px 16px; border: 1.5px solid {border}; border-radius: 10px; background: {bg}; cursor: pointer;">
  <input type="radio" name="source" value="{title}"{chk} style="width: 18px; height: 18px; accent-color: #0F766E; margin: 3px 0 0;">
  <div style="display: flex; flex-direction: column; gap: 4px; flex-grow: 1; min-width: 0;">
    <div style="display: flex; align-items: center; gap: 8px; font-size: 15px; font-weight: 600;"><span style="display: inline-flex; gap: 2px;">{icons}</span><span>{title}</span>{bd}</div>
    <div class="help">{desc}</div>
    {microw}
  </div>
</label>"""

live_setup_main = header("Live") + f"""
<div style="flex-grow: 1; display: flex; align-items: flex-start; justify-content: center; padding: 28px 24px; overflow: hidden;">
  <div class="card" style="width: 680px; display: flex; flex-direction: column; gap: 22px; padding: 28px;">
    <div style="display: flex; flex-direction: column; gap: 4px;">
      <h2 class="h2" style="font-size: 18px;">Bắt đầu phiên Live</h2>
      <p class="muted" style="margin: 0;">Ghi âm liên tục từ lúc bắt đầu; transcript và bản dịch chạy realtime. Đổi nguồn, đổi ngôn ngữ dịch giữa chừng không ngắt phiên.</p>
    </div>
    <div style="display: flex; flex-direction: column; gap: 10px;">
      <div class="label">Nguồn âm thanh</div>
      {source_option(["mic","monitor"], "Mic + Hệ thống", "Thu giọng bạn và âm thanh máy (Zoom, Teams, Meet…). Cả hai đều được transcribe và ghi âm.", True, "Khuyên dùng", True)}
      {source_option(["monitor"], "Chỉ hệ thống", "Chỉ âm thanh phát ra từ máy. Giọng bạn không được ghi.")}
      {source_option(["mic"], "Chỉ mic", "Chỉ microphone. Dùng khi họp trực tiếp trong phòng.")}
    </div>
    <div class="field">
      <label class="label" for="live-tag">Tag <span class="help" style="font-weight: 400;">(tuỳ chọn)</span> <span class="tip">{ic("help",14)}</span></label>
      <div style="display: flex; align-items: center; gap: 6px; flex-wrap: wrap; min-height: 36px; padding: 4px 6px; border: 1px solid #CFD4DA; border-radius: 8px; background: #FFFFFF;">
        <button class="chip chip-on" type="button" aria-label="Bỏ tag KH-ABC">KH-ABC {ic("x",12)}</button>
        <button class="chip chip-on" type="button" aria-label="Bỏ tag sprint-12">sprint-12 {ic("x",12)}</button>
        <input id="live-tag" class="input" type="text" placeholder="Gõ để tìm hoặc thêm tag…" style="border: 0; height: 28px; width: 220px; padding: 0 6px; background: transparent;">
      </div>
      <div style="display: flex; align-items: center; gap: 6px; flex-wrap: wrap;">
        <span class="help">Gần đây:</span>
        <button class="chip chip-sm" type="button">KH-XYZ</button>
        <button class="chip chip-sm" type="button">sales</button>
        <button class="chip chip-sm" type="button">nội bộ</button>
        <button class="chip chip-sm" type="button">demo</button>
      </div>
      <div class="help">Phiên sẽ vào đúng bộ lọc ngay khi lưu; sửa tag sau ở Trang chủ hoặc Transcript detail.</div>
    </div>
    <div style="display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px;">
      <div class="field">
        <label class="label" for="lang-sel">Ngôn ngữ transcribe <span class="tip">{ic("help",14)}</span></label>
        <select id="lang-sel" class="select"><option>Tự nhận diện (auto)</option><option>日本語</option><option>Tiếng Việt</option><option>English</option></select>
        <div class="help">Auto cho phép “Nhận diện lại” khi buổi họp đổi ngôn ngữ.</div>
      </div>
      <div class="field">
        <label class="label" for="target-sel">Dịch sang <span class="tip">{ic("help",14)}</span></label>
        <select id="target-sel" class="select"><option>Tiếng Việt</option><option>English</option><option>日本語</option><option>Không dịch</option></select>
        <div class="help">Bản dịch chỉ hiển thị trong Live, không lưu vào phiên.</div>
      </div>
    </div>
    <div style="display: flex; align-items: flex-start; gap: 10px; padding: 12px 14px; border-radius: 10px; background: #F6F6F2; color: #3C4551; font-size: 13px;">
      {ic("shield",16,"#0F766E")}<span>macOS sẽ hỏi quyền <strong>System Audio Recording</strong> và <strong>Micro</strong> lần đầu. trans-kun không bao giờ xin quyền Screen Recording. Âm thanh app tự phát (đọc bản dịch) không bị thu lại.</span>
    </div>
    <div style="display: flex; align-items: center; justify-content: space-between; padding-top: 6px; border-top: 1px solid #EDEFF1;">
      <span class="help">Phím tắt: <span class="kbd">⌘</span> <span class="kbd">⇧</span> <span class="kbd">L</span> bắt đầu / dừng</span>
      <a class="btn btn-primary btn-lg" href="Live.dc.html">{ic("radio",18,"#FFFFFF")}<span>Bắt đầu ghi</span></a>
    </div>
  </div>
</div>"""

# ---------------------------------------------------------------- live running
def live_line(ts, text, streaming=False):
    car = '<span class="caret"></span>' if streaming else ""
    return f'<div class="seg" style="cursor: default;"><span class="ts">{ts}</span><span class="seg-text">{text}{car}</span></div>'

live_main = f"""
<div style="display: flex; align-items: center; gap: 10px; height: 64px; padding: 0 20px; border-bottom: 1px solid #E1E4E8; background: #FFFFFF; flex: none;">
  <span class="status status-rec" role="status"><span class="dot dot-pulse"></span><span>Đang ghi</span><span class="mono" style="font-weight: 500;">00:45:12</span></span>
  <span class="status status-warn" role="status">{ic("wifi-off",15,"#8A4B0A")}<span>Đang nối lại</span><span class="mono" style="font-weight: 500;">2:10</span></span>
  <span style="flex-grow: 1;"></span>
  <div class="segmented" role="group" aria-label="Chế độ xem">
    <button class="seg-btn" type="button">Gốc</button>
    <button class="seg-btn" type="button">Dịch</button>
    <button class="seg-btn seg-btn-on" type="button" aria-pressed="true">Cả hai</button>
  </div>
  <div style="width: 1px; height: 24px; background: #E1E4E8; margin: 0 4px;"></div>
  <select class="select" aria-label="Nguồn âm thanh" style="width: 170px;"><option>Mic + Hệ thống</option><option>Chỉ hệ thống</option><option>Chỉ mic</option></select>
  <select class="select" aria-label="Ngôn ngữ dịch" style="width: 150px;"><option>→ Tiếng Việt</option><option>→ English</option><option>→ 日本語</option><option>Không dịch</option></select>
  <button class="btn btn-secondary" type="button" title="Mở kết nối mới với ngữ cảnh trống để bắt lại ngôn ngữ đang nói">{ic("languages",16)}<span>Nhận diện lại</span></button>
  <button class="btn btn-secondary btn-icon" type="button" aria-label="Đọc bản dịch: đang bật" aria-pressed="true" style="border-color: #0F766E; color: #0B5D57; background: #E6F3F1;">{ic("speaker",16,"#0B5D57")}</button>
  <button class="btn btn-secondary btn-icon" type="button" aria-label="Ghi chú" aria-pressed="true" style="border-color: #0F766E; color: #0B5D57; background: #E6F3F1;">{ic("edit",16,"#0B5D57")}</button>
  <a class="btn btn-danger" href="Transcript.dc.html">{ic("stop",16,"#FFFFFF")}<span>Dừng</span></a>
</div>

<div style="display: flex; align-items: center; gap: 10px; padding: 8px 20px; background: #FDF0DC; border-bottom: 1px solid #F5D9A8; color: #8A4B0A; font-size: 13px; flex: none;">
  {ic("info",15,"#8A4B0A")}<span><strong>Mất mạng từ 45:02.</strong> Vẫn đang ghi âm; transcript sẽ tự chạy tiếp khi có mạng. Bạn có thể Transcribe lại sau để lấp khoảng thiếu.</span>
</div>

<div style="flex-grow: 1; display: flex; min-height: 0;">
  <section style="flex-grow: 1; min-width: 0; display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); background: #FFFFFF; border-right: 1px solid #E1E4E8;">
    <div style="display: flex; flex-direction: column; gap: 2px; padding: 8px; border-right: 1px solid #EDEFF1; overflow: hidden;">
      <div style="display: flex; align-items: center; gap: 8px; padding: 4px 12px 8px; font-size: 12px; font-weight: 600; color: #5B6470; text-transform: uppercase; letter-spacing: .04em;">{ic("mic",14)}<span>Gốc</span><span class="badge badge-file">ja · auto</span></div>
      {live_line("00:43:10", "それから、ダッシュボードのグラフは週次と月次の切り替えが必要です。")}
      {live_line("00:43:24", "デフォルトは週次でお願いします。")}
      {live_line("00:43:38", "了解です。月次はカレンダーの選択で対応します。")}
      {live_line("00:44:02", "あと、通知メールのテンプレートは日本語と英語の両方が必要になります。")}
      {live_line("00:44:31", "ベトナム語は今回のスコープ外で大丈夫です。")}
      {live_line("00:44:50", "では、テンプレートは二言語で作成し、後から追加できる構成にします。")}
      <div style="display: flex; align-items: center; gap: 10px; padding: 8px 12px; margin: 4px 0; border-radius: 8px; background: #ECEDE8; color: #3C4551; font-size: 12px; font-weight: 600;">{ic("wifi-off",14)}<span>Mất kết nối 45:02 – đang chờ…</span></div>
    </div>
    <div style="display: flex; flex-direction: column; gap: 2px; padding: 8px; overflow: hidden;">
      <div style="display: flex; align-items: center; gap: 8px; padding: 4px 12px 8px; font-size: 12px; font-weight: 600; color: #5B6470; text-transform: uppercase; letter-spacing: .04em;">{ic("languages",14)}<span>Dịch</span><span class="badge badge-file">→ vi</span><span class="status status-ok" style="height: 22px; font-size: 11px; padding: 0 8px; margin-left: auto;">{ic("speaker",12,"#0B5D57")}<span>Đang đọc</span></span></div>
      {live_line("00:43:10", "Ngoài ra, biểu đồ trên dashboard cần chuyển được giữa theo tuần và theo tháng.")}
      {live_line("00:43:24", "Mặc định xin để theo tuần.")}
      {live_line("00:43:38", "Đã rõ. Theo tháng sẽ xử lý bằng chọn trên lịch.")}
      {live_line("00:44:02", "Ngoài ra, template email thông báo sẽ cần cả tiếng Nhật lẫn tiếng Anh.")}
      {live_line("00:44:31", "Tiếng Việt nằm ngoài phạm vi lần này cũng được.")}
      {live_line("00:44:50", "Vậy template sẽ làm hai ngôn ngữ, cấu trúc cho phép thêm sau", True)}
    </div>
  </section>
  <aside style="width: 320px; flex: none; display: flex; flex-direction: column; background: #FAFAF7;">
    <div style="display: flex; align-items: center; justify-content: space-between; height: 44px; padding: 0 14px; border-bottom: 1px solid #E1E4E8; flex: none;">
      <div class="label">{ic("edit",14)}Ghi chú</div>
      <span class="help" style="display: inline-flex; align-items: center; gap: 4px;">{ic("check",12,"#0B5D57")}Đã lưu 14:44</span>
    </div>
    <label for="notes" style="position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0);">Ghi chú phiên</label>
    <textarea id="notes" class="input" style="flex-grow: 1; border: 0; border-radius: 0; background: transparent; font-size: 14px; padding: 14px;">- Export CSV: giữ filter, chỉ quyền edit+
- Dashboard: tuần (mặc định) / tháng
- Email template: ja + en, vi để sau
- Hỏi lại: giới hạn 10k dòng khi export?</textarea>
    <div style="padding: 10px 14px; border-top: 1px solid #E1E4E8; flex: none;" class="help">Ghi chú được nhúng vào prompt khi sinh memo. Tự lưu, không mất khi force-quit.</div>
  </aside>
</div>"""

# ---------------------------------------------------------------- settings
def setting(label, control, help_text, idfor):
    return f"""<div style="display: grid; grid-template-columns: 220px minmax(0, 1fr); gap: 20px; padding: 16px 0; border-top: 1px solid #EDEFF1; align-items: start;">
  <label class="label" for="{idfor}" style="padding-top: 8px;">{label} <span class="tip">{ic("help",14)}</span></label>
  <div style="display: flex; flex-direction: column; gap: 6px;">{control}<div class="help">{help_text}</div></div>
</div>"""

def snav(name, icon, active=False):
    cls = "nav-item nav-item-active" if active else "nav-item"
    href = {"Gemini": "Settings.dc.html", "Memo": "SettingsMemo.dc.html"}.get(name, f"#{name}")
    return f'<a class="{cls}" href="{href}">{ic(icon,16)}<span>{name}</span></a>'

settings_main = header("Cài đặt") + f"""
<div style="flex-grow: 1; display: flex; min-height: 0;">
  <nav aria-label="Nhóm cài đặt" style="width: 220px; flex: none; display: flex; flex-direction: column; gap: 2px; padding: 16px 12px; border-right: 1px solid #E1E4E8; background: #FAFAF7;">
    {snav("Chung","sliders")}{snav("Gemini","key",True)}{snav("Chunking","scissors")}{snav("Live","radio")}{snav("Memo","memo")}{snav("Lưu trữ","database")}{snav("Chẩn đoán","activity")}{snav("Cấu hình đề xuất","cloud")}
    <div style="height: 1px; background: #E1E4E8; margin: 8px 4px;"></div>
    {snav("Giới thiệu & Quyền riêng tư","shield")}
  </nav>
  <div style="flex-grow: 1; min-width: 0; padding: 20px 32px 24px; display: flex; flex-direction: column; gap: 28px; overflow: hidden;">
    <section style="display: flex; flex-direction: column;">
      <div style="display: flex; flex-direction: column; gap: 2px; padding-bottom: 8px;"><h2 class="h2">Gemini</h2><div class="help">Key và model dùng cho transcribe file, live và memo. Key lưu trong Keychain / Credential Manager.</div></div>
      {setting("API key", f'''<div style="display: flex; gap: 8px;">
        <div style="position: relative; flex-grow: 1; display: flex;"><input id="s-key" class="input mono" type="password" value="AIzaSyD4k1xxxxxxxxxxxxxxxxxxxxxx, AQ.Ab8RN6xxxxxxxxxxxxxxxx" style="padding-right: 40px;"><button class="btn btn-ghost btn-icon btn-sm" type="button" aria-label="Hiện key" style="position: absolute; right: 3px; top: 3px;">{ic("eye",16)}</button></div>
        <button class="btn btn-secondary" type="button">Kiểm tra key</button></div>
        <div role="status" style="display: inline-flex; align-items: center; gap: 8px; font-size: 13px; color: #0B5D57; font-weight: 500;">{ic("check",14,"#0B5D57",2.2)}2 key hợp lệ · 14 model khả dụng · kiểm tra lúc 14:02</div>''',
        "Nhiều key cách nhau dấu phẩy. 429 → key nghỉ 60 s và xoay sang key kế; 401/403 → loại key tới khi bạn sửa.", "s-key")}
      {setting("Model transcribe file", f'''<div style="display: flex; gap: 8px;"><select id="s-m1" class="select" style="flex-grow: 1;"><option>gemini-flash-lite-latest</option><option>gemini-2.5-pro</option><option>gemini-3-transcribe</option></select><button class="btn btn-secondary" type="button">{ic("refresh",14)}<span>Tải danh sách</span></button></div>''',
        "Model chuyên transcribe (tên có “-transcribe”) trả word timestamp và speaker; app tự chọn cách gọi.", "s-m1")}
      {setting("Model live", '<select id="s-m2" class="select"><option>gemini-3.5-live-translate-preview</option><option>gemini-2.5-flash-native-audio</option></select>', "Chỉ liệt kê model hỗ trợ Live. Tên ngoài danh sách vẫn được chấp nhận, kèm cảnh báo.", "s-m2")}
      {setting("Model memo", '<select id="s-m3" class="select"><option>gemini-flash-lite-latest</option><option>gemini-2.5-pro</option></select>', "Dùng khi sinh memo từ transcript + ghi chú.", "s-m3")}
      {setting("Ngôn ngữ transcribe", '<select id="s-lang" class="select" style="width: 260px;"><option>Tự nhận diện (auto)</option><option>日本語</option><option>Tiếng Việt</option><option>English</option></select>', "Áp dụng cho cả Transcribe file và Live.", "s-lang")}
    </section>
    <section style="display: flex; flex-direction: column;">
      <div style="display: flex; flex-direction: column; gap: 2px; padding-bottom: 8px;"><h2 class="h2">Lưu trữ</h2><div class="help">Toàn bộ dữ liệu nằm trong Container do store cấp, không tuỳ chỉnh thư mục.</div></div>
      <div style="display: grid; grid-template-columns: 220px minmax(0, 1fr); gap: 20px; padding: 16px 0; border-top: 1px solid #EDEFF1; align-items: start;">
        <div class="label" style="padding-top: 2px;">Dung lượng đang dùng</div>
        <div style="display: flex; flex-direction: column; gap: 10px;">
          <div style="display: flex; height: 10px; border-radius: 5px; overflow: hidden; background: #ECEDE8;"><div style="width: 62%; background: #0F766E;"></div><div style="width: 3%; background: #1E4E9B;"></div></div>
          <div style="display: flex; gap: 18px; font-size: 13px;">
            <span style="display: inline-flex; align-items: center; gap: 6px;"><span class="dot" style="color: #0F766E;"></span>Media (proxy, recording) <span class="mono">1,24 GB</span></span>
            <span style="display: inline-flex; align-items: center; gap: 6px;"><span class="dot" style="color: #1E4E9B;"></span>Cơ sở dữ liệu <span class="mono">8,1 MB</span></span>
            <span class="muted">38 phiên</span>
          </div>
          <div style="display: flex; gap: 8px;">
            <button class="btn btn-secondary btn-sm" type="button">{ic("folder",14)}<span>Mở thư mục</span></button>
            <button class="btn btn-danger-soft btn-sm" type="button">{ic("trash",14)}<span>Xoá toàn bộ dữ liệu…</span></button>
          </div>
        </div>
      </div>
    </section>
    <section style="display: flex; flex-direction: column;">
      <div style="display: flex; flex-direction: column; gap: 2px; padding-bottom: 8px;"><h2 class="h2">Chẩn đoán</h2><div class="help">Nhật ký không bao giờ chứa transcript, bản dịch, ghi chú, memo hay key.</div></div>
      <div style="display: grid; grid-template-columns: 220px minmax(0, 1fr); gap: 20px; padding: 16px 0; border-top: 1px solid #EDEFF1; align-items: center;">
        <div class="label">Nhật ký chẩn đoán</div>
        <div style="display: flex; gap: 8px; align-items: center;">
          <button class="btn btn-secondary btn-sm" type="button">{ic("download",14)}<span>Xuất gói nhật ký</span></button>
          <button class="btn btn-ghost btn-sm" type="button">Xoá nhật ký</button>
          <span class="help mono">3 file · 412 KB</span>
        </div>
      </div>
      <div style="display: grid; grid-template-columns: 220px minmax(0, 1fr); gap: 20px; padding: 16px 0; border-top: 1px solid #EDEFF1; align-items: center;">
        <label class="label" for="s-stats">Gửi thống kê ẩn danh</label>
        <div style="display: flex; gap: 10px; align-items: center;">
          <input id="s-stats" type="checkbox" style="width: 18px; height: 18px; accent-color: #0F766E; margin: 0;">
          <span class="help">Mặc định tắt. Chỉ số phiên, số lỗi theo loại, số crash — không nội dung, không định danh.</span>
        </div>
      </div>
    </section>
  </div>
</div>"""

# ---------------------------------------------------------------- settings — memo templates
def tpl_item(name, meta, selected=False, default=False):
    bg = "#E6F3F1" if selected else "transparent"
    col = "#0B5D57" if selected else "#171A1F"
    bd = '<span class="badge badge-file">Mặc định · vi</span>' if default else '<span class="badge badge-memo">Của bạn</span>'
    cur = ' aria-current="true"' if selected else ""
    return f"""<a href="#tpl" class="nav-item" style="height: auto; padding: 10px 12px; background: {bg}; color: {col}; align-items: flex-start; gap: 10px;"{cur}>
  {ic("grip",14,"#8A919C")}
  <div style="display: flex; flex-direction: column; gap: 4px; flex-grow: 1; min-width: 0;">
    <div style="font-size: 14px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{name}</div>
    <div style="display: flex; align-items: center; gap: 8px;">{bd}<span class="help">{meta}</span></div>
  </div>
</a>"""

settings_memo_main = header("Cài đặt") + f"""
<div style="flex-grow: 1; display: flex; min-height: 0;">
  <nav aria-label="Nhóm cài đặt" style="width: 220px; flex: none; display: flex; flex-direction: column; gap: 2px; padding: 16px 12px; border-right: 1px solid #E1E4E8; background: #FAFAF7;">
    {snav("Chung","sliders")}{snav("Gemini","key")}{snav("Chunking","scissors")}{snav("Live","radio")}{snav("Memo","memo",True)}{snav("Lưu trữ","database")}{snav("Chẩn đoán","activity")}{snav("Cấu hình đề xuất","cloud")}
    <div style="height: 1px; background: #E1E4E8; margin: 8px 4px;"></div>
    {snav("Giới thiệu & Quyền riêng tư","shield")}
  </nav>
  <div style="flex-grow: 1; min-width: 0; padding: 20px 32px 24px; display: flex; flex-direction: column; gap: 16px; overflow: hidden;">
    <div style="display: flex; align-items: flex-start; justify-content: space-between; gap: 16px;">
      <div style="display: flex; flex-direction: column; gap: 2px;"><h2 class="h2">Memo — mẫu prompt</h2><div class="help">Giữ nhiều mẫu cho từng loại cuộc họp; chọn mẫu khi bấm “Sinh memo” ở Transcript detail. Mẫu mặc định đi theo ngôn ngữ UI (vi/en/ja).</div></div>
      <button class="btn btn-secondary" type="button">+ Thêm mẫu</button>
    </div>
    <div style="display: grid; grid-template-columns: 300px minmax(0, 1fr); gap: 20px; flex-grow: 1; min-height: 0;">
      <div class="card" style="display: flex; flex-direction: column; overflow: hidden;">
        <div style="display: flex; flex-direction: column; gap: 2px; padding: 8px; flex-grow: 1;">
          {tpl_item("議事録 (mặc định)", "dùng 24 lần", True, True)}
          {tpl_item("Action items", "dùng 9 lần", False, True)}
          {tpl_item("Tóm tắt 5 dòng", "dùng 3 lần", False, True)}
          {tpl_item("Biên bản họp sales", "sửa 12/09", False, False)}
          {tpl_item("Q&amp;A với khách (BrSE)", "sửa 03/09", False, False)}
        </div>
        <div style="padding: 10px 12px; border-top: 1px solid #EDEFF1; display: flex; flex-direction: column; gap: 6px;">
          <button class="btn btn-ghost btn-sm" type="button" style="justify-content: flex-start;">{ic("refresh",14)}<span>Khôi phục mẫu mặc định (vi)</span></button>
          <div class="help">Chỉ ghi lại 3 mẫu mặc định; mẫu của bạn không bị đụng.</div>
        </div>
      </div>
      <div class="card" style="display: flex; flex-direction: column; overflow: hidden;">
        <div style="display: flex; flex-direction: column; gap: 14px; padding: 18px 20px; flex-grow: 1; min-height: 0;">
          <div class="field">
            <label class="label" for="tpl-name">Tên mẫu</label>
            <input id="tpl-name" class="input" type="text" value="議事録 (mặc định)" style="width: 360px;">
          </div>
          <div class="field" style="flex-grow: 1; min-height: 0;">
            <label class="label" for="tpl-prompt">Prompt <span class="tip">{ic("help",14)}</span></label>
            <textarea id="tpl-prompt" class="input mono" style="flex-grow: 1; min-height: 300px; font-size: 13px; line-height: 1.6;">あなたは日本語とベトナム語の会議に同席したアシスタントです。以下の文字起こしから議事録を作成してください。

## 出力形式（Markdown）
- 参加者
- 決定事項
- Action items（担当・期限）
- 未解決の論点

参加者のメモがあれば優先して反映してください。

## 文字起こし
{{transcript}}

## 参加者のメモ
{{notes}}</textarea>
            <div style="display: flex; align-items: center; gap: 14px; flex-wrap: wrap;">
              <span style="display: inline-flex; align-items: center; gap: 6px; font-size: 12px; font-weight: 600; color: #0B5D57;">{ic("check",14,"#0B5D57",2.2)}<span class="mono">{{transcript}}</span><span style="font-weight: 400;">bắt buộc · có</span></span>
              <span style="display: inline-flex; align-items: center; gap: 6px; font-size: 12px; font-weight: 600; color: #0B5D57;">{ic("check",14,"#0B5D57",2.2)}<span class="mono">{{notes}}</span><span style="font-weight: 400;">tuỳ chọn · có</span></span>
              <span class="help">Thiếu <span class="mono">{{transcript}}</span> thì không lưu được. Nếu bỏ <span class="mono">{{notes}}</span>, ghi chú được nối vào cuối prompt.</span>
            </div>
          </div>
        </div>
        <div style="display: flex; align-items: center; gap: 8px; padding: 12px 20px; border-top: 1px solid #EDEFF1; background: #FAFAF7;">
          <button class="btn btn-danger-soft btn-sm" type="button" aria-disabled="true" title="Mẫu mặc định không xoá được, chỉ sửa hoặc khôi phục">{ic("trash",14)}<span>Xoá mẫu</span></button>
          <span class="help">Mẫu mặc định: sửa được, không xoá được.</span>
          <span style="flex-grow: 1;"></span>
          <button class="btn btn-ghost" type="button">Huỷ thay đổi</button>
          <button class="btn btn-primary" type="button">Lưu mẫu</button>
        </div>
      </div>
    </div>
  </div>
</div>"""

# ---------------------------------------------------------------- components sheet
def swatch(hexv, name, role):
    return f'<div style="display: flex; flex-direction: column; gap: 6px; width: 132px;"><div class="sw" style="background: {hexv}; width: 100%; height: 44px;"></div><div style="font-size: 12px; font-weight: 600;">{name}</div><div class="help mono" style="font-size: 11px;">{hexv} · {role}</div></div>'

def err(cat, msg, action):
    return f"""<div style="display: flex; align-items: flex-start; gap: 10px; padding: 10px 12px; border-radius: 8px; background: #FFFFFF; border: 1px solid #E1E4E8; width: 380px;">
  {ic("alert",16,"#B42318")}
  <div style="display: flex; flex-direction: column; gap: 2px; flex-grow: 1;"><div style="font-size: 13px; font-weight: 600;">{cat}</div><div class="help">{msg}</div></div>
  <button class="btn btn-secondary btn-sm" type="button">{action}</button>
</div>"""

def block(title, inner):
    return f'<div style="display: flex; flex-direction: column; gap: 12px;"><div style="font-size: 12px; font-weight: 700; color: #5B6470; text-transform: uppercase; letter-spacing: .06em;">{title}</div>{inner}</div>'

components = f"""<div style="width: 1280px; height: 900px; padding: 32px; background: #F6F6F2; display: flex; flex-direction: column; gap: 28px; overflow: hidden;">
  <div style="display: flex; align-items: baseline; gap: 12px;"><h1 class="h1">Hệ thống thiết kế trans-kun v3</h1><span class="muted">Token, trạng thái và thành phần dùng chung · light mode</span></div>
  {block("Màu", f'''<div style="display: flex; gap: 14px; flex-wrap: wrap;">
    {swatch("#F6F6F2","bg","nền app")}{swatch("#F0F0EB","bg-sidebar","sidebar")}{swatch("#FFFFFF","surface","card, list")}{swatch("#E1E4E8","border","viền")}
    {swatch("#171A1F","text","chữ chính")}{swatch("#5B6470","text-secondary","phụ · 5.9:1")}{swatch("#0F766E","accent","teal · 5.0:1")}{swatch("#E6F3F1","accent-soft","chọn/active")}
    {swatch("#B42318","danger","dừng, xoá")}{swatch("#8A4B0A","warning","partial, nối lại")}{swatch("#1E4E9B","info","audio")}{swatch("#5B2FA3","recover","phục hồi")}
  </div>''')}
  <div style="display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 40px;">
    <div style="display: flex; flex-direction: column; gap: 28px;">
      {block("Chỉ báo Live — ghi âm và kết nối là hai chỉ báo riêng", f'''<div style="display: flex; flex-direction: column; gap: 10px;">
        <div style="display: flex; gap: 10px; align-items: center;"><span class="status status-rec"><span class="dot dot-pulse"></span><span>Đang ghi</span><span class="mono" style="font-weight: 500;">00:45:12</span></span><span class="help">luôn hiển thị khi phiên chạy, không phụ thuộc mạng</span></div>
        <div style="display: flex; gap: 10px; align-items: center;"><span class="status status-ok">{ic("wifi",15,"#0B5D57")}<span>Đang transcribe</span></span><span class="help">có kết nối model sống</span></div>
        <div style="display: flex; gap: 10px; align-items: center;"><span class="status status-warn">{ic("wifi-off",15,"#8A4B0A")}<span>Đang nối lại</span><span class="mono" style="font-weight: 500;">2:10</span></span><span class="help">backoff 1→30 s, không giới hạn số lần</span></div>
        <div style="display: flex; gap: 10px; align-items: center;"><span class="status status-off">{ic("x",15)}<span>Đã dừng transcript</span></span><span class="help">server từ chối setup 5 lần · vẫn ghi âm</span></div>
      </div>''')}
      {block("Badge phiên", f'''<div style="display: flex; gap: 8px; flex-wrap: wrap;">{B_MEMO}{B_AUDIO}{B_PART}{B_REC}<span class="badge badge-live">LIVE</span><span class="badge badge-file">FILE</span><span class="badge badge-token">{ic("zap",11,"#8A4B0A")} Tốn token Gemini</span></div>''')}
      {block("Nút", f'''<div style="display: flex; gap: 8px; flex-wrap: wrap; align-items: center;">
        <button class="btn btn-primary" type="button">Chính</button><button class="btn btn-secondary" type="button">Phụ</button><button class="btn btn-ghost" type="button">Nhẹ</button><button class="btn btn-danger" type="button">{ic("stop",16,"#FFFFFF")}<span>Dừng</span></button><button class="btn btn-danger-soft" type="button">Xoá</button><button class="btn btn-primary" type="button" aria-disabled="true">Cần key</button><button class="btn btn-secondary btn-icon" type="button" aria-label="Copy">{ic("copy",16)}</button>
      </div>
      <div class="help">Cao 36 px (44 px cho bước lớn), bo 8 px, viền 1 px, không đổ bóng. Focus: ring 2 px teal. Nút cần Gemini bị vô hiệu kèm tooltip “Chưa có key” và lối tắt tới Cài đặt.</div>''')}
    </div>
    <div style="display: flex; flex-direction: column; gap: 28px;">
      {block("Lỗi có category — luôn kèm hành động", f'''<div style="display: flex; flex-direction: column; gap: 8px;">
        {err("Quota (429)", "Mọi key đang nghỉ. Job chờ tối đa 180 s/chunk rồi thử lại.", "Chờ")}
        {err("Key bị từ chối (401/403)", "Không còn key hợp lệ. Sửa key trong Cài đặt.", "Mở Cài đặt")}
        {err("Model không tồn tại", "gemini-3.5-live-translate-preview bị Google trả lỗi. Chọn model khác.", "Chọn model")}
        {err("Mạng / CA công ty", "Không xác minh được chứng chỉ TLS (proxy?). Dùng CA store hệ thống.", "Hướng dẫn")}
        {err("Định dạng không hỗ trợ", "File .wmv — hãy chuyển sang mp4 / m4a / mp3.", "Đóng")}
        {err("Thiếu quyền hệ thống", "Cần System Audio Recording. Cấp trong System Settings.", "Mở Settings")}
      </div>''')}
      {block("Ad slot — chỉ Home, Transcript, Cài đặt; không bao giờ ở Live", f'''<div style="display: flex; gap: 16px; align-items: flex-start;">
        <div style="width: 236px; display: flex; flex-direction: column; gap: 6px;">
          <div style="display: flex; align-items: center; justify-content: space-between; font-size: 11px; color: #5B6470;"><span style="font-weight: 600; letter-spacing: .04em; text-transform: uppercase;">Sponsored</span>{ic("help",14,"#8A919C")}</div>
          <div style="border-radius: 10px; overflow: hidden; border: 1px solid #E1E4E8; background: #FFFFFF;"><div style="height: 64px; background: #E6F3F1; display: flex; align-items: center; justify-content: center;">{ic("image",22,"#0B5D57")}</div><div style="padding: 8px 10px; font-size: 12px; font-weight: 600;">Creative 300×100 hoặc 320×50</div></div>
          <span style="font-size: 11px; color: #5B6470; display: inline-flex; align-items: center; gap: 4px;">{ic("flag",12)}Báo cáo quảng cáo</span>
        </div>
        <div class="help" style="width: 200px;">Nhãn “Sponsored”, nút Báo cáo (mailto kèm id creative), “Vì sao tôi thấy quảng cáo này” (text tĩnh). Không tracking, không âm thanh, không che nội dung. Ẩn khi <span class="mono">ads_enabled=false</span> hoặc <span class="mono">is_premium</span>.</div>
      </div>''')}
    </div>
  </div>
</div>"""

# ---------------------------------------------------------------- tag picker popover
def tag_item(name, count, checked=False):
    chk = ' checked="checked"' if checked else ""
    return f"""<label style="display: flex; align-items: center; gap: 10px; height: 36px; padding: 0 10px; border-radius: 8px; cursor: pointer;">
  <input type="checkbox"{chk} style="width: 16px; height: 16px; accent-color: #0F766E; margin: 0;">
  <span style="flex-grow: 1; font-size: 14px;">{name}</span>
  <span class="mono help">{count}</span>
</label>"""

tag_picker = f"""<div style="width: 420px; height: 520px; padding: 24px; background: #F6F6F2; display: flex; flex-direction: column; gap: 12px;">
  <div class="help">Popover mở từ chip “+ N tag khác” ở Trang chủ · cũng dùng cho “+ Tag” ở Transcript detail và ô Tag ở Live setup</div>
  <div role="dialog" aria-label="Chọn tag" class="card" style="display: flex; flex-direction: column; overflow: hidden; box-shadow: 0 10px 32px rgba(17,24,39,.12);">
    <div style="padding: 10px; border-bottom: 1px solid #EDEFF1;">
      <div style="position: relative; display: flex;">
        <span style="position: absolute; left: 10px; top: 9px; color: #5B6470;">{ic("search",16)}</span>
        <input class="input" type="search" placeholder="Tìm tag… (Enter để tạo mới)" aria-label="Tìm tag" style="padding-left: 34px;">
      </div>
    </div>
    <div style="display: flex; flex-direction: column; gap: 2px; padding: 8px;">
      <div style="display: flex; align-items: center; justify-content: space-between; padding: 6px 10px 4px; font-size: 11px; font-weight: 700; color: #5B6470; text-transform: uppercase; letter-spacing: .06em;"><span>Đang lọc · 2</span><button class="btn btn-ghost btn-sm" type="button" style="height: 24px; font-size: 12px;">Bỏ chọn hết</button></div>
      {tag_item("KH-ABC", "12", True)}
      {tag_item("sprint-12", "6", True)}
      <div style="padding: 10px 10px 4px; font-size: 11px; font-weight: 700; color: #5B6470; text-transform: uppercase; letter-spacing: .06em;">Tất cả · 14 · theo số phiên</div>
      {tag_item("KH-XYZ", "9")}
      {tag_item("sales", "7")}
      {tag_item("nội bộ", "5")}
      {tag_item("demo", "4")}
      {tag_item("kickoff", "3")}
      {tag_item("sprint-11", "3")}
    </div>
    <div style="display: flex; align-items: center; justify-content: space-between; padding: 10px 14px; border-top: 1px solid #EDEFF1; background: #FAFAF7;">
      <span class="help">Nhiều tag = AND · tối đa 20 tag/phiên</span>
      <a href="#manage" style="font-size: 13px; font-weight: 600; text-decoration: none; display: inline-flex; align-items: center; gap: 4px;">{ic("tag",14)}Quản lý tag</a>
    </div>
  </div>
</div>"""

# ---------------------------------------------------------------- write files
# ---------------------------------------------------------------- dark theme
# Mỗi token light ánh xạ sang ĐÚNG token dark đã thiết kế trong
# DESIGN.md / MASTER.md — đây không phải phép đảo màu.
DARK_MAP = {
  "#F6F6F2": "#141517",  # bg
  "#F0F0EB": "#1A1B1E",  # bg-sidebar
  "#FAFAF7": "#1A1B1E",  # panel nền phụ
  "#F7F7F4": "#1F2024",
  "#FFFFFF": "#1F2024",  # surface (chữ/icon trắng được bảo vệ riêng bên dưới)
  "#ECEDE8": "#2A2B30",  # surface-sunken
  "#F1F2EE": "#26282D",  # segment hover
  "#EDEFF1": "#26282D",  # viền chia dòng
  "#E1E4E8": "#2E3036",  # border
  "#CFD4DA": "#3C3F47",  # border-strong
  "#171A1F": "#ECEDEF",  # text
  "#3C4551": "#C2C7CF",  # text-secondary
  "#5B6470": "#A0A6B0",  # text-muted
  "#8A919C": "#A0A6B0",
  "#0B5D57": "#5EEAD4",  # accent-hover dùng làm chữ
  "#E6F3F1": "#16302E",  # accent-soft
  "#F3FAF9": "#132725",
  "#B7DDD8": "#2F5F5A",  # accent-border
  "#E0F2F1": "#12302D",  # memo-soft
  "#8A4B0A": "#F5B75C",  # warning
  "#FDF0DC": "#33291A",  # warning-soft
  "#F5D9A8": "#4A3A1F",  # warning-border
  "#FFF7E6": "#2B2413",  # token-soft
  "#FFFBF2": "#2B2413",
  "#B42318": "#F87171",  # danger
  "#A11E1E": "#FCA5A5",  # danger-strong
  "#8E1C13": "#FCA5A5",
  "#FDE8E8": "#35201F",  # danger-soft
  "#FDF1F0": "#35201F",
  "#F0B8B3": "#5A3330",  # danger-border
  "#1E4E9B": "#7AA7F0",  # info
  "#E8EEF7": "#1C2636",  # info-soft
  "#5B2FA3": "#C4A6F5",  # recover
  "#F0EAFB": "#262036",  # recover-soft
  "#FDE68A": "#7A6019",  # mark (đo 5.10:1 với #ECEDEF)
}
# accent #0F766E giữ nguyên khi là NỀN nút primary; chuyển sang #2DD4BF khi là chữ/icon/viền.
_ACCENT_TEXT = [
  ("color: #0F766E", "color: @@AC@@"), ("color:#0F766E", "color:@@AC@@"),
  ("border-color: #0F766E", "border-color: @@AC@@"), ("border-color:#0F766E", "border-color:@@AC@@"),
  ('stroke="#0F766E"', 'stroke="@@AC@@"'),
]
# chữ/icon trắng trên nền đặc phải Ở NGUYÊN màu trắng
_KEEP_WHITE = [
  ("color: #FFFFFF", "color: @@W@@"), ("color:#FFFFFF", "color:@@W@@"),
  ('stroke="#FFFFFF"', 'stroke="@@W@@"'),
]

def to_dark(html):
    for a, b in _ACCENT_TEXT + _KEEP_WHITE:
        html = html.replace(a, b)
    html = html.replace("background:#0B5D57", "background:#0F766E").replace("background: #0B5D57", "background: #0F766E")
    for light, dark in DARK_MAP.items():
        html = html.replace(light, dark)
        html = html.replace(light.lower(), dark)
    html = html.replace("@@AC@@", "#2DD4BF").replace("@@W@@", "#FFFFFF")
    html = html.replace("<html lang=", '<html data-theme="dark" lang=', 1)
    return html

# ------------------------------------------------- Live: overlay đang lưu phiên
saving_overlay = f"""
<div role="status" style="position: absolute; inset: 0; background: rgba(23,26,31,.45); display: flex; align-items: center; justify-content: center;">
  <div style="width: 380px; background: #FFFFFF; border-radius: 16px; box-shadow: 0 10px 32px rgba(17,24,39,.12); padding: 28px; display: flex; flex-direction: column; align-items: center; gap: 12px; text-align: center;">
    <div style="width: 40px; height: 40px; border-radius: 999px; background: #E6F3F1; display: flex; align-items: center; justify-content: center;">{ic("check", 20, "#0B5D57", 2.2)}</div>
    <div style="font-size: 16px; font-weight: 600;">Đang lưu phiên…</div>
    <div class="help" style="max-width: 280px;">Finalize recording, tạo proxy phát lại. Xong sẽ mở Transcript detail của phiên vừa tạo.</div>
  </div>
</div>"""

def overlaid(inner, w, h, overlay):
    return f'<div style="position: relative; width: {w}px; height: {h}px;">{inner}{overlay}</div>'

boards = {
  "OnboardingLang.dc.html":    ("Onboarding — Ngôn ngữ", ob_lang, 1280, 800, True),
  "OnboardingConsent.dc.html": ("Onboarding — Đồng ý dữ liệu", ob_consent, 1280, 800, True),
  "OnboardingKey.dc.html":     ("Onboarding — API key", ob_key, 1280, 800, True),
  "HomeEmpty.dc.html":         ("Home — trống", shell("home", home_empty_main, job=False), 1280, 800, True),
  "Home.dc.html":              ("Home — thư viện phiên", shell("home", home_main), 1280, 800, True),
  "TagPicker.dc.html":         ("Tag picker (popover)", tag_picker, 420, 520, False),
  "Transcript.dc.html":        ("Transcript detail", shell("home", transcript_main, h=860), 1280, 860, True),
  "Settings.dc.html":          ("Cài đặt", shell("settings", settings_main, h=1000), 1280, 1000, True),
  "SettingsMemo.dc.html":      ("Cài đặt — Memo (mẫu prompt)", shell("settings", settings_memo_main, h=1000), 1280, 1000, True),
  "LiveSetup.dc.html":         ("Live — bắt đầu", shell("live", live_setup_main, h=920, show_ad=False), 1280, 920, True),
  "Live.dc.html":              ("Live — đang ghi", shell("live", live_main, show_ad=False), 1280, 800, True),
  "LiveSaving.dc.html":        ("Live — đang lưu phiên", overlaid(shell("live", live_main, show_ad=False), 1280, 800, saving_overlay), 1280, 800, False),
  "Components.dc.html":        ("Thành phần & trạng thái", components, 1280, 900, False),
}
def _write(name, html):
    with open(os.path.join(ROOT, name), "w", encoding="utf-8") as f:
        f.write(html)

for name, (title, body, w, h, inter) in list(boards.items()):
    _write(name, page(title, body, w, h))

# Dark mode nằm trong bản submit đầu (chốt 2026-09-18) -> sinh 4 màn chủ chốt.
DARK_SOURCES = ["Home.dc.html", "Transcript.dc.html", "Live.dc.html", "Components.dc.html"]
for src in DARK_SOURCES:
    title, body, w, h, inter = boards[src]
    dname = src.replace(".dc.html", "Dark.dc.html")
    dtitle = title + " (dark)"
    _write(dname, to_dark(page(dtitle, body, w, h)))
    boards[dname] = (dtitle, body, w, h, False)

GAP = 80
rows = [
  ("Onboarding — 3 bước, không kiểm tra môi trường", ["OnboardingLang.dc.html","OnboardingConsent.dc.html","OnboardingKey.dc.html"]),
  ("Home — thư viện phiên", ["HomeEmpty.dc.html","Home.dc.html","TagPicker.dc.html"]),
  ("Transcript detail & Cài đặt", ["Transcript.dc.html","Settings.dc.html","SettingsMemo.dc.html"]),
  ("Live — không có Ad slot", ["LiveSetup.dc.html","Live.dc.html","LiveSaving.dc.html"]),
  ("Thành phần dùng chung", ["Components.dc.html"]),
  ("Dark mode — trong bản submit đầu", ["HomeDark.dc.html","TranscriptDark.dc.html","LiveDark.dc.html","ComponentsDark.dc.html"]),
]
bj, order, notes = {}, [], {}
y = 0
for i, (title, names) in enumerate(rows):
    x = 0
    rowh = 0
    for n in names:
        t, _, w, h, inter = boards[n]
        e = {"x": x, "y": y, "w": w, "h": h, "title": t}
        if inter: e["is_interactive"] = True
        bj[n] = e; order.append(n)
        x += w + GAP; rowh = max(rowh, h)
    notes[f"row{i}"] = {"x": 0, "y": y - 240, "text": title, "kind": "title1", "maxW": x - GAP}
    y += rowh + 120 + 240

canvas = {
  "v": 3,
  "createdOnFiles": {"v": 1, "at": "2026-09-18T01:30:14Z"},
  "title": "trans-kun v3 UI Redesign",
  "launch": {"view": "canvas"},
  "pages": [],
  "boards": bj,
  "order": order,
  "notes": notes,
  "designSystems": [],
}
with open(os.path.join(ROOT, "canvas.json"), "w", encoding="utf-8") as f:
    json.dump(canvas, f, ensure_ascii=False, indent=1)
print("wrote", len(boards), "artboards")
