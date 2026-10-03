# Lautta canvas — Silica style kit (shared by every artboard)

Goal: every artboard looks like a native Sailfish OS 5 Silica app. NO iOS/Android
mannerisms: no tab bars, no bottom nav, no FABs, no hamburger, no toolbars with
titles on the left, no back arrows/buttons, no cards with drop shadows, no
toggle-pill switches, no modal sheets sliding from bottom, no snackbars with
"UNDO" caps, no Material ripples, no checkboxes, no chevrons ">" on list rows,
no separators between list rows, no centred titles.

Silica idioms to use instead:
- PageHeader: right-aligned, light weight, highlight colour; description below it.
- Back navigation = swipe right; shown only by the page-stack indicator (top-left
  small dots `.ind-back`). Attached page (swipe left) = `.ind-fwd` top-right.
- Actions = PulleyMenu (top, pull down) and PushUpMenu (bottom). Closed state shows
  only the thin glowing `.pull-ind` line at the top edge (`.push-ind` at bottom).
  Open state: menu items centred, primary colour, the item under the finger in
  highlight colour, highlight-tinted gradient backdrop; page content pushed down.
- Per-item actions = long-press ContextMenu that expands INLINE below the item,
  pushing following rows down (`.cm`). The pressed row gets `.li.pressed`.
- Destructive = RemorseItem in the row (`.remorse`) or RemorsePopup at page top.
- Dialogs = full pages with DialogHeader: "Cancel" left (small), accept text right
  (large, highlight). Accept by swiping left too. No OK/Cancel buttons at bottom.
- Multi-select = tap icons; actions in a DockedPanel at bottom (`.dock`) of
  icon-only IconButtons.
- Settings controls: TextSwitch (glowing dot left of text, NOT a pill toggle),
  ComboBox (label + highlight value; opens inline menu), Slider (value above,
  label below), TextField (underline, label below).
- Empty states = ViewPlaceholder (big light highlight text centred + hint).
- Busy = BusyIndicator ring. Progress = thin highlight line.
- Buttons (rare in Silica; only in page content) = `.btn` rounded.
- Section headers = small, right-aligned, highlight colour.
- Hints for first-use = InteractionHintLabel at bottom (`.hintlabel`).

## Sizes (Theme, scaled to a 400 px wide phone)
pageMargin 18 · paddingSmall 6 · paddingMedium 12 · paddingLarge 18
itemSizeSmall 56 · itemSizeMedium 72 · itemSizeLarge 90 (header 80)
fontSizeTiny 12 · ExtraSmall 13 · Small 15 · Medium 19 · Large 26 · ExtraLarge 34 · Huge 46
iconSizeSmall 24 · iconSizeMedium 40 · iconSizeLarge 64

Phone frame 400×860 (portrait); landscape 860×400. A frame may be taller (e.g.
400×1400) to show a whole scrolled page; then also set the `$preview` height.

## Skeleton of every artboard

```html
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>NAME</title>
<script src="./support.js"></script>
</head>
<body>
<x-dc>
<helmet>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link href="https://fonts.googleapis.com/css2?family=Source+Sans+3:wght@300;400;600&amp;family=Source+Code+Pro:wght@400&amp;display=swap" rel="stylesheet">
<style>
...PASTE THE CSS BELOW VERBATIM...
</style>
</helmet>
<div class="amb {{amb}}" style="width: 400px; height: 860px">
  ... content ...
</div>
</x-dc>
<script type="text/x-dc" data-dc-script data-props='{"ambience":{"editor":"enum","options":["dark","light"],"default":"dark"},"$preview":{"width":400,"height":860}}'>
class Component extends DCLogic {
renderVals() {
return { amb: (this.props.ambience ?? 'dark') === 'light' ? 'light' : 'dark' };
}
}
</script>
</body>
</html>
```

Rules: keep `<script src="./support.js"></script>` exactly; close every element;
quote every attribute; `{{hole}}` is a plain lookup only; all copy is literal markup;
no emoji; icons inline `<svg class="ic" viewBox="0 0 24 24">…</svg>` (stroke icons
below); real `<button>`/`<a href>`/`<input>`/`<label>` for controls, `aria-label` on
icon-only buttons; no `<iframe>`; no scripts building DOM. Use `&amp;` for & in
attribute values, `&lt;` `&gt;` in text.

## CSS (paste verbatim in each artboard's helmet `<style>`)

```css
body{margin:0}
.amb{--pri:#ffffff;--sec:rgba(255,255,255,.62);--hl:#74d4ff;--hl2:#5fb3d9;--hlbg:rgba(116,212,255,.30);--hldim:rgba(116,212,255,.13);--ov:rgba(8,14,20,.55);--line:rgba(255,255,255,.28);--btn:rgba(255,255,255,.16);--warn:#ffb067;
 background:radial-gradient(130% 70% at 85% -5%,#2a4a63 0%,#13222f 45%,#0a1219 100%);
 color:var(--pri);font-family:'Source Sans 3',system-ui,sans-serif;font-size:19px;font-weight:400;position:relative;overflow:hidden;box-sizing:border-box}
.amb.light{--pri:#15191d;--sec:rgba(21,25,29,.66);--hl:#00639c;--hl2:#2a74a3;--hlbg:rgba(0,99,156,.20);--hldim:rgba(0,99,156,.09);--ov:rgba(240,244,247,.7);--line:rgba(21,25,29,.3);--btn:rgba(21,25,29,.10);--warn:#a14d00;
 background:radial-gradient(130% 70% at 85% -5%,#ffffff 0%,#e7eef3 50%,#d6e1e9 100%)}
.amb *{box-sizing:border-box}
.amb button{font:inherit;color:inherit;background:none;border:0;padding:0;margin:0;cursor:pointer;text-align:inherit}
.amb a{color:inherit;text-decoration:none}
.ic{width:24px;height:24px;fill:none;stroke:currentColor;stroke-width:1.5;stroke-linecap:round;stroke-linejoin:round;flex:none}
.ic.m{width:40px;height:40px;stroke-width:1.2}
.ic.l{width:64px;height:64px;stroke-width:1}
.hl{color:var(--hl)}.sec{color:var(--sec)}.hl2{color:var(--hl2)}.warn{color:var(--warn)}
.small{font-size:15px}.xs{font-size:13px}.tiny{font-size:12px}.large{font-size:26px;font-weight:300}.xl{font-size:34px;font-weight:300}
.mono{font-family:'Source Code Pro',monospace}
/* page-stack indicators */
.ind-back,.ind-fwd{position:absolute;top:14px;display:flex;gap:4px}
.ind-back{left:10px}.ind-fwd{right:10px}
.ind-back i,.ind-fwd i{display:block;width:4px;height:4px;border-radius:2px;background:var(--hl);opacity:.9}
.ind-back i+i,.ind-fwd i+i{opacity:.45}
/* pulley/push-up indicators (closed) */
.pull-ind{position:absolute;top:0;left:0;right:0;height:3px;background:linear-gradient(90deg,transparent,var(--hl) 50%,transparent);opacity:.7}
.push-ind{position:absolute;bottom:0;left:0;right:0;height:3px;background:linear-gradient(90deg,transparent,var(--hl) 50%,transparent);opacity:.7}
/* open pulley / push-up menu */
.pulley{padding:18px 18px 6px;background:linear-gradient(180deg,var(--hlbg),var(--hldim) 70%,transparent);display:flex;flex-direction:column}
.pushup{padding:6px 18px 18px;background:linear-gradient(0deg,var(--hlbg),var(--hldim) 70%,transparent);display:flex;flex-direction:column}
.mi{height:56px;display:flex;align-items:center;justify-content:center;font-size:19px;color:var(--pri);width:100%;text-align:center}
.mi.on{color:var(--hl)}
.mi .cnt{margin-left:8px;color:var(--sec);font-size:15px}
.mi.on .cnt{color:var(--hl2)}
.mi.dis{color:var(--sec);opacity:.5}
/* page header */
.hdr{min-height:80px;padding:14px 18px 6px 60px;display:flex;flex-direction:column;align-items:flex-end;justify-content:center;text-align:right}
.hdr-t{font-size:28px;font-weight:300;color:var(--hl);line-height:1.15}
.hdr-d{font-size:15px;color:var(--hl2);margin-top:2px}
/* dialog header */
.dlg{height:80px;padding:0 18px;display:flex;align-items:center;justify-content:space-between}
.dlg-c{font-size:19px;color:var(--pri)}
.dlg-a{font-size:28px;font-weight:300;color:var(--hl)}
.dlg-a.dis{opacity:.4}
.dlg-sub{padding:0 18px 8px;text-align:right;font-size:15px;color:var(--hl2)}
/* section header */
.sh{padding:14px 18px 6px;text-align:right;font-size:15px;color:var(--hl)}
/* list item */
.li{min-height:72px;padding:6px 18px;display:flex;align-items:center;gap:14px;width:100%}
.li.s{min-height:56px}
.li.pressed{background:var(--hlbg)}
.li.selected{background:var(--hldim)}
.li-ic{width:48px;height:48px;display:flex;align-items:center;justify-content:center;flex:none;color:var(--pri);position:relative}
.li-ic.sel::after{content:"";position:absolute;inset:2px;border:2px solid var(--hl);border-radius:3px}
.thumb{width:48px;height:48px;flex:none;background:linear-gradient(135deg,#4b6b7f,#9bb7a0);}
.li-b{flex:1;min-width:0}
.li-t{font-size:19px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.li-s{font-size:15px;color:var(--sec);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.li-r{font-size:15px;color:var(--sec);flex:none;text-align:right}
.li.pressed .li-t{color:var(--hl)}
/* context menu (inline, under pressed item) */
.cm{background:var(--hldim);display:flex;flex-direction:column;padding:4px 0}
/* badges / dots */
.dot{width:8px;height:8px;border-radius:4px;background:var(--hl);display:inline-block;flex:none}
.dot.off{background:transparent;border:1px solid var(--sec)}
.badge{font-size:13px;color:var(--warn);display:inline-flex;align-items:center;gap:4px}
.tagdot{width:12px;height:12px;border-radius:6px;display:inline-block;flex:none}
.lossy{font-size:12px;border:1px solid var(--sec);color:var(--sec);border-radius:3px;padding:0 4px;margin-left:6px;vertical-align:middle}
/* progress */
.prog{height:3px;background:var(--line);position:relative;overflow:hidden}
.prog>i{position:absolute;left:0;top:0;bottom:0;background:var(--hl);display:block}
.slim{position:absolute;left:0;right:0;height:2px;background:linear-gradient(90deg,transparent,var(--hl),transparent);opacity:.85}
/* busy indicator */
.busy{width:64px;height:64px;border-radius:50%;border:3px solid var(--hldim);border-top-color:var(--hl);border-right-color:var(--hl)}
.busy.s{width:24px;height:24px;border-width:2px}
/* placeholder */
.ph{position:absolute;left:18px;right:18px;text-align:center}
.ph-t{font-size:34px;font-weight:300;color:var(--hl);opacity:.85;line-height:1.2}
.ph-h{font-size:19px;color:var(--hl2);margin-top:12px;line-height:1.35}
/* buttons */
.btn{min-width:150px;height:52px;padding:0 26px;border-radius:26px;background:var(--btn);color:var(--pri);font-size:19px;display:inline-flex;align-items:center;justify-content:center}
.btn.down{background:var(--hlbg);color:var(--hl)}
.btns{display:flex;gap:18px;justify-content:center;flex-wrap:wrap;padding:12px 18px}
.ibtn{width:56px;height:56px;display:inline-flex;align-items:center;justify-content:center;color:var(--pri)}
.ibtn.on{color:var(--hl)}
/* docked panel */
.dock{position:absolute;left:0;right:0;bottom:0;min-height:80px;background:var(--ov);backdrop-filter:blur(6px);border-top:1px solid var(--hldim);display:flex;align-items:center;justify-content:space-around;padding:0 6px}
/* text switch */
.sw{display:flex;gap:14px;padding:10px 18px;align-items:flex-start;width:100%}
.sw-d{width:18px;height:18px;border-radius:9px;border:1.5px solid var(--sec);margin-top:5px;flex:none}
.sw.on .sw-d{border-color:var(--hl);background:var(--hl);box-shadow:0 0 10px 2px var(--hlbg)}
.sw-t{font-size:19px}.sw-x{font-size:15px;color:var(--sec);margin-top:2px;line-height:1.3}
/* combo box */
.cb{display:flex;flex-direction:column;padding:10px 18px;width:100%}
.cb-r{display:flex;gap:10px;align-items:baseline;flex-wrap:wrap}
.cb-l{font-size:19px}.cb-v{font-size:19px;color:var(--hl)}
.cb-x{font-size:15px;color:var(--sec);margin-top:2px}
/* slider */
.sl{padding:8px 18px 12px}
.sl-v{text-align:center;font-size:26px;font-weight:300;color:var(--hl)}
.sl-t{height:2px;background:var(--line);margin:14px 6px;position:relative}
.sl-t>i{position:absolute;left:0;top:0;bottom:0;background:var(--hl)}
.sl-t>b{position:absolute;top:-9px;width:20px;height:20px;border-radius:10px;background:var(--hl);box-shadow:0 0 12px 3px var(--hlbg);margin-left:-10px}
.sl-l{text-align:center;font-size:15px;color:var(--sec)}
/* text field */
.tf{padding:10px 18px}
.tf input,.tf textarea{width:100%;background:transparent;border:0;border-bottom:1px solid var(--line);color:var(--pri);font:inherit;font-size:19px;padding:6px 0;outline:none}
.tf.focus input{border-bottom-color:var(--hl)}
.tf label{display:block;font-size:15px;color:var(--sec);margin-top:4px}
.tf.focus label{color:var(--hl)}
.tf.err input{border-bottom-color:var(--warn)}
.tf.err label{color:var(--warn)}
/* remorse */
.remorse{min-height:72px;background:var(--hlbg);position:relative;padding:10px 18px;display:flex;flex-direction:column;justify-content:center;overflow:hidden}
.remorse>i{position:absolute;left:0;top:0;bottom:0;background:var(--hldim);display:block}
.remorse b{font-weight:400;font-size:19px;position:relative}
.remorse span{font-size:15px;color:var(--sec);position:relative}
/* banner (in-page notice, e.g. offline / undo) */
.banner{margin:6px 18px;padding:12px 14px;background:var(--hldim);display:flex;gap:12px;align-items:center;font-size:15px}
.banner .act{color:var(--hl);font-size:15px;margin-left:auto;flex:none}
/* interaction hint label */
.hintlabel{position:absolute;left:0;right:0;bottom:0;padding:40px 30px 30px;text-align:center;font-size:19px;color:var(--hl);background:linear-gradient(0deg,var(--ov),transparent)}
/* key/value detail (DetailItem) */
.di{display:flex;gap:12px;padding:5px 18px;font-size:15px}
.di-l{flex:1;text-align:right;color:var(--sec)}
.di-v{flex:1;color:var(--pri);word-break:break-all}
/* notification popup (system) */
.note{background:rgba(20,30,40,.92);color:#fff;padding:12px 16px;display:flex;gap:12px;align-items:flex-start;border-radius:6px}
```

## Stroke icons (inner markup for `<svg class="ic" viewBox="0 0 24 24">…</svg>`)

folder `<path d="M3 6.5h6l2 2h10v10.5H3z"/>`
file `<path d="M6 3h8l4 4v14H6zM14 3v4h4"/>`
text-doc `<path d="M6 3h8l4 4v14H6zM14 3v4h4M9 12h6M9 15h6M9 18h4"/>`
image `<rect x="3" y="5" width="18" height="14"/><circle cx="9" cy="10" r="1.6"/><path d="M3 17l5-5 4 4 3-3 6 6"/>`
music `<path d="M9 18V6l10-2v12"/><circle cx="7" cy="18" r="2"/><circle cx="17" cy="16" r="2"/>`
video `<rect x="3" y="6" width="13" height="12"/><path d="M16 10l5-3v10l-5-3"/>`
archive `<rect x="4" y="3" width="16" height="18"/><path d="M12 3v2M12 7v2M12 11v2"/><rect x="10.5" y="14" width="3" height="4"/>`
database `<ellipse cx="12" cy="6" rx="7" ry="3"/><path d="M5 6v12c0 1.7 3.1 3 7 3s7-1.3 7-3V6M5 12c0 1.7 3.1 3 7 3s7-1.3 7-3"/>`
server `<rect x="3" y="4" width="18" height="7"/><rect x="3" y="13" width="18" height="7"/><path d="M7 7.5h.01M7 16.5h.01"/>`
sd-card `<path d="M7 3h8l4 4v14H7zM10 3v4M13 3v4"/>`
usb `<path d="M12 3v14M9 6l3-3 3 3M7 10v3l5 3M17 9v3l-5 3"/><circle cx="12" cy="19" r="2"/>`
phone(android) `<rect x="7" y="3" width="10" height="18" rx="2"/><path d="M11 18h2"/>`
nearby `<circle cx="12" cy="12" r="2"/><path d="M8 8a6 6 0 0 0 0 8M16 8a6 6 0 0 1 0 8M5 5a10 10 0 0 0 0 14M19 5a10 10 0 0 1 0 14"/>`
star `<path d="M12 3l2.7 5.6 6.1.9-4.4 4.3 1 6.1L12 17l-5.4 2.9 1-6.1L3.2 9.5l6.1-.9z"/>`
tag `<path d="M3 12V3h9l9 9-9 9z"/><circle cx="7.5" cy="7.5" r="1.5"/>`
search `<circle cx="10.5" cy="10.5" r="6.5"/><path d="M15.5 15.5L21 21"/>`
upload `<path d="M12 16V4M7 9l5-5 5 5M4 20h16"/>`
download `<path d="M12 4v12M7 11l5 5 5-5M4 20h16"/>`
across `<path d="M4 8h14l-3-3M20 16H6l3 3"/>`
pause `<path d="M8 5v14M16 5v14"/>`
play `<path d="M7 4l13 8-13 8z"/>`
close `<path d="M6 6l12 12M18 6L6 18"/>`
check `<path d="M4 12l5 5L20 6"/>`
more `<path d="M5 12h.01M12 12h.01M19 12h.01" stroke-width="3"/>`
copy `<rect x="8" y="8" width="12" height="12"/><path d="M16 8V4H4v12h4"/>`
cut `<circle cx="6" cy="18" r="2.5"/><circle cx="18" cy="18" r="2.5"/><path d="M8 16L18 4M16 16L6 4"/>`
paste `<rect x="5" y="4" width="14" height="17"/><path d="M9 4V2h6v2"/>`
delete `<path d="M4 7h16M9 7V4h6v3M6 7l1 14h10l1-14"/>`
share `<circle cx="18" cy="5" r="2.5"/><circle cx="6" cy="12" r="2.5"/><circle cx="18" cy="19" r="2.5"/><path d="M8.2 10.8l7.6-4.4M8.2 13.2l7.6 4.4"/>`
info `<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7.5h.01"/>`
edit `<path d="M4 20h4L20 8l-4-4L4 16z"/>`
refresh `<path d="M20 12a8 8 0 1 1-2.3-5.7M20 4v5h-5"/>`
sync `<path d="M4 9h13l-3-3M20 15H7l3 3"/>`
lock `<rect x="5" y="11" width="14" height="10"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/>`
warning `<path d="M12 3l10 18H2zM12 10v5M12 18h.01"/>`
link `<path d="M10 14l4-4M8 11l-3 3a3 3 0 0 0 4 4l3-3M16 13l3-3a3 3 0 0 0-4-4l-3 3"/>`
clock `<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>`
settings `<circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9L7 7M17 17l2.1 2.1M4.9 19.1L7 17M17 7l2.1-2.1"/>`
select `<rect x="4" y="4" width="16" height="16"/><path d="M8 12l3 3 5-6"/>`
grid `<rect x="4" y="4" width="7" height="7"/><rect x="13" y="4" width="7" height="7"/><rect x="4" y="13" width="7" height="7"/><rect x="13" y="13" width="7" height="7"/>`
list `<path d="M4 6h16M4 12h16M4 18h16"/>`
restore `<path d="M4 12a8 8 0 1 0 2.3-5.7M4 4v5h5"/>`
eye `<path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/>`
pin `<path d="M9 3h6l-1 6 4 4H6l4-4zM12 13v8"/>`

## Content conventions
- App name "Lautta". Sample data: realistic, no lorem ipsum. Servers: "NAS" (SFTP,
  nas.home), "Office" (WebDAV), ad-hoc "ftp.kotisivu.fi". Volume "SD card".
- Sizes "4.2 MB", times "Today 09:14", "Yesterday", "12 Sep".
- Everything Finnish-flavoured is fine but English UI copy.
- Never show bridge features in artboards labelled "standalone".
