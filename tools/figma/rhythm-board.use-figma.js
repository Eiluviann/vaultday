const ICONS = __ICONS__;
const SPHERE_ICON = { work: 'briefcase', life: 'leaf', everywhere: 'globe' };
for (const style of ['Regular', 'Medium', 'SemiBold', 'Bold']) await figma.loadFontAsync({ family: 'Geist', style });
const vars = {};
for (const v of await figma.variables.getLocalVariablesAsync('COLOR')) vars[v.name] = v;
const created = [];

function paint(name, opacity) {
  const v = vars[name];
  if (!v) throw new Error('no variable ' + name);
  const p = figma.variables.setBoundVariableForPaint({ type: 'SOLID', color: { r: 0, g: 0, b: 0 } }, 'color', v);
  return opacity === undefined ? p : Object.assign({}, p, { opacity });
}
const SHADOW = [{ type: 'DROP_SHADOW', color: { r: 27 / 255, g: 38 / 255, b: 51 / 255, a: 0.12 }, offset: { x: 0, y: 1 }, radius: 2, spread: 0, visible: true, blendMode: 'NORMAL' }];
function box(dir, props) {
  const f = figma.createAutoLayout(dir);
  f.fills = [];
  if (props) f.set(props);
  created.push(f.id);
  return f;
}
function height(f, h) { f.resize(f.width, h); f.primaryAxisSizingMode = 'AUTO'; f.counterAxisSizingMode = 'FIXED'; return f; }
function fixed(f, w, h) { f.resize(w, h); f.primaryAxisSizingMode = 'FIXED'; f.counterAxisSizingMode = 'FIXED'; return f; }
function spacer(name) { const s = figma.createFrame(); s.name = name || 'spacer'; s.fills = []; s.resize(1, 1); return s; }
function edge(f, side, color) {
  f.strokes = [paint(color || 'line')]; f.strokeAlign = 'INSIDE';
  f.strokeTopWeight = side === 'top' ? 1 : 0; f.strokeBottomWeight = side === 'bottom' ? 1 : 0;
  f.strokeLeftWeight = side === 'left' ? 1 : 0; f.strokeRightWeight = side === 'right' ? 1 : 0;
}
function text(chars, o) {
  o = o || {};
  const t = figma.createText();
  t.fontName = { family: 'Geist', style: o.weight || 'Regular' };
  t.characters = chars;
  t.fontSize = o.size || 13;
  t.lineHeight = { unit: 'PIXELS', value: o.lh || 18 };
  if (o.ls) t.letterSpacing = { unit: 'PERCENT', value: o.ls };
  t.fills = [paint(o.color || 'ink')];
  if (o.width) { t.resize(o.width, 10); t.textAutoResize = 'HEIGHT'; }
  if (o.align) t.textAlignHorizontal = o.align;
  t.name = o.name || chars.slice(0, 32);
  return t;
}
function icon(name, size, color, transform) {
  const g = ICONS[name];
  if (!g) throw new Error('no icon ' + name);
  const v = g.vb.split(' ').map(Number);
  const side = Math.max(v[2], v[3]), pad = side / 8, b = side + pad * 2;
  const vb = [v[0] - (side - v[2]) / 2 - pad, v[1] - (side - v[3]) / 2 - pad, b, b].join(' ');
  const tr = transform ? ' transform="' + transform + '"' : '';
  const n = figma.createNodeFromSvg('<svg xmlns="http://www.w3.org/2000/svg" width="' + size + '" height="' + size + '" viewBox="' + vb + '"><path fill="#000"' + tr + ' d="' + g.d + '"/></svg>');
  n.name = 'icon/' + name;
  n.resize(size, size);
  n.fills = [];
  for (const c of n.findAll(x => x.type === 'VECTOR')) c.fills = [paint(color || 'ink-muted')];
  return n;
}
function fill(node, h, v) {
  if (h !== false) node.layoutSizingHorizontal = 'FILL';
  if (v) node.layoutSizingVertical = 'FILL';
  return node;
}

// ---- controls -------------------------------------------------------------
function button(o) {
  const sm = o.size === 'sm';
  const iconOnly = o.icon && !o.label;
  const b = box('HORIZONTAL', { name: 'Button · ' + (o.label || o.icon), itemSpacing: 6, cornerRadius: 6, primaryAxisAlignItems: 'CENTER', counterAxisAlignItems: 'CENTER',
    paddingLeft: iconOnly ? 0 : sm ? 8 : 16, paddingRight: iconOnly ? 0 : sm ? 8 : 16 });
  const h = sm ? 22 : 28;
  const primary = o.variant === 'primary';
  b.fills = [paint(primary ? 'primary' : 'surface-raised')];
  if (!primary) { b.strokes = [paint('line-button')]; b.strokeWeight = 1; b.strokeAlign = 'INSIDE'; }
  const inkColor = primary ? 'on-primary' : 'ink';
  if (o.icon) b.appendChild(icon(o.icon, sm ? 13 : 14, inkColor));
  if (o.label) b.appendChild(text(o.label, { size: sm ? 12 : 13, weight: primary ? 'SemiBold' : 'Medium', color: inkColor }));
  if (o.iconAfter) b.appendChild(icon(o.iconAfter, sm ? 13 : 14, inkColor));
  if (iconOnly) fixed(b, h, h); else height(b, h);
  return b;
}
function segmented(o) {
  const seg = box('HORIZONTAL', { name: 'Segmented control · ' + o.options.join(' | '), paddingTop: 2, paddingBottom: 2, paddingLeft: 2, paddingRight: 2, itemSpacing: 2, cornerRadius: 8 });
  seg.fills = [paint(o.ground === 'sunken' ? 'fill-control' : 'fill-control-soft')];
  for (const opt of o.options) {
    const on = opt === o.selected;
    const b = box('HORIZONTAL', { name: 'Option · ' + opt, paddingLeft: 8, paddingRight: 8, cornerRadius: 6, primaryAxisAlignItems: 'CENTER', counterAxisAlignItems: 'CENTER' });
    if (on) { b.fills = [paint('surface-raised')]; b.effects = SHADOW; }
    b.appendChild(text(opt, { size: 12, lh: 16, weight: on ? 'SemiBold' : 'Regular', color: on ? 'ink' : 'ink-secondary' }));
    height(b, 22);
    seg.appendChild(b);
    if (o.fill) b.layoutSizingHorizontal = 'FILL';
  }
  return seg;
}
function toggle(on) {
  const t = figma.createFrame(); t.name = 'Toggle · ' + (on ? 'on' : 'off'); t.resize(38, 22); t.cornerRadius = 999;
  t.fills = [paint(on ? 'primary' : 'fill-control')];
  if (!on) { t.strokes = [paint('line-control')]; t.strokeWeight = 1; t.strokeAlign = 'INSIDE'; }
  const knob = figma.createEllipse(); knob.name = 'Knob'; knob.resize(18, 18); knob.x = on ? 18 : 2; knob.y = 2;
  knob.fills = [paint(on ? 'on-primary' : 'surface')]; knob.effects = SHADOW;
  t.appendChild(knob);
  return t;
}
function field(value, width) {
  const f = box('HORIZONTAL', { name: 'Time field · ' + value, paddingLeft: 8, paddingRight: 8, cornerRadius: 6, primaryAxisAlignItems: 'MAX', counterAxisAlignItems: 'CENTER' });
  f.fills = [paint('surface-raised')]; f.strokes = [paint('line-control')]; f.strokeWeight = 1; f.strokeAlign = 'INSIDE';
  f.appendChild(text(value, { size: 12, lh: 16 }));
  fixed(f, width, 26);
  return f;
}
function select(value) {
  const s = box('HORIZONTAL', { name: 'Select · ' + value, paddingLeft: 6, paddingRight: 4, itemSpacing: 6, cornerRadius: 6, counterAxisAlignItems: 'CENTER' });
  s.fills = [paint('surface-raised')]; s.strokes = [paint('line-control')]; s.strokeWeight = 1; s.strokeAlign = 'INSIDE';
  const t = s.appendChild(text(value, { size: 12, lh: 16 }));
  s.appendChild(spacer());
  s.children[1].layoutSizingHorizontal = 'FILL';
  const cap = box('HORIZONTAL', { name: 'Disclosure', cornerRadius: 5, primaryAxisAlignItems: 'CENTER', counterAxisAlignItems: 'CENTER' });
  cap.fills = [paint('primary')];
  cap.appendChild(icon('chevron.left', 11, 'on-primary', 'rotate(-90 160 256)'));
  fixed(cap, 16, 16);
  s.appendChild(cap);
  height(s, 26);
  return s;
}
function checkbox(label, checked) {
  const c = box('HORIZONTAL', { name: 'Checkbox · ' + label, itemSpacing: 4, counterAxisAlignItems: 'CENTER' });
  const sq = box('HORIZONTAL', { name: 'Box', cornerRadius: 3.5, primaryAxisAlignItems: 'CENTER', counterAxisAlignItems: 'CENTER' });
  if (checked) { sq.fills = [paint('primary')]; sq.appendChild(icon('checkmark', 9, 'on-primary')); }
  else { sq.fills = [paint('surface-raised')]; sq.strokes = [paint('line-control')]; sq.strokeWeight = 1; sq.strokeAlign = 'INSIDE'; }
  fixed(sq, 14, 14);
  c.appendChild(sq);
  c.appendChild(text(label, { size: 12, lh: 16 }));
  return c;
}

// ---- composites -----------------------------------------------------------
function panel(sphere, title) {
  const p = box('VERTICAL', { name: 'Panel · ' + title, paddingTop: 12, paddingBottom: 12, paddingLeft: 12, paddingRight: 12, itemSpacing: 8, cornerRadius: 16 });
  p.fills = [paint('surface-sunken')]; p.strokes = [paint('line')]; p.strokeWeight = 1; p.strokeAlign = 'INSIDE';
  const head = box('HORIZONTAL', { name: 'Panel head', paddingTop: 2, paddingBottom: 4, paddingLeft: 2, paddingRight: 2, itemSpacing: 8, counterAxisAlignItems: 'CENTER' });
  const mark = box('HORIZONTAL', { name: 'Mark', cornerRadius: 6, primaryAxisAlignItems: 'CENTER', counterAxisAlignItems: 'CENTER' });
  const tone = sphere === 'work' ? 'work' : sphere === 'life' ? 'life' : null;
  mark.fills = [paint(tone ? tone + '-soft' : 'fill-control')];
  mark.appendChild(icon(SPHERE_ICON[sphere], 14, tone || 'ink-muted'));
  fixed(mark, 22, 22);
  head.appendChild(mark);
  head.appendChild(text(title, { weight: 'Bold', color: tone || 'ink' }));
  p.appendChild(head); fill(head);
  return p;
}
function card(o) {
  const single = !o.desc && !o.extra;
  const c = box('VERTICAL', { name: 'Card · ' + o.title, paddingTop: 12, paddingBottom: single ? 12 : 20, paddingLeft: 12, paddingRight: 12, itemSpacing: 0, cornerRadius: 12 });
  c.fills = [paint('surface-raised')]; c.strokes = [paint('line')]; c.strokeWeight = 1; c.strokeAlign = 'INSIDE';
  const head = box('HORIZONTAL', { name: 'Head', itemSpacing: 0, counterAxisAlignItems: 'CENTER' });
  const iconBox = box('HORIZONTAL', { name: 'Icon box', primaryAxisAlignItems: 'CENTER', counterAxisAlignItems: 'CENTER' });
  iconBox.appendChild(icon(o.icon, 16, o.tone || 'ink-muted'));
  fixed(iconBox, 18, 18);
  const iconWrap = box('HORIZONTAL', { name: 'Icon', paddingLeft: 4, paddingRight: 8 });
  iconWrap.appendChild(iconBox);
  head.appendChild(iconWrap);
  head.appendChild(text(o.title, { size: 14, lh: 18, weight: 'SemiBold', name: 'Title' }));
  fill(head.children[1]);
  if (o.trailing) { const tr = box('HORIZONTAL', { name: 'Trailing', paddingLeft: 12, itemSpacing: 8, counterAxisAlignItems: 'CENTER' }); tr.appendChild(o.trailing); head.appendChild(tr); }
  height(head, Math.max(26, head.height));
  c.appendChild(head); fill(head);
  if (!single) {
    const body = box('VERTICAL', { name: 'Body', paddingLeft: 30, itemSpacing: 0 });
    if (o.desc) {
      const d = box('HORIZONTAL', { name: 'Description', paddingTop: 8 });
      d.appendChild(text(o.desc, { size: 12, lh: 16, color: 'ink-secondary', width: 200 }));
      fill(d.children[0]);
      body.appendChild(d); fill(d);
    }
    if (o.extra) {
      const e = box('HORIZONTAL', { name: 'Controls', paddingTop: 12 });
      e.appendChild(o.extra); fill(o.extra);
      body.appendChild(e); fill(e);
    }
    c.appendChild(body); fill(body);
  }
  return c;
}
function stepRow(n, label, state) {
  const r = box('HORIZONTAL', { name: 'Step ' + n + ' · ' + label, paddingTop: 5, paddingBottom: 5, paddingLeft: 8, paddingRight: 8, itemSpacing: 8, cornerRadius: 8, counterAxisAlignItems: 'CENTER' });
  const mark = box('HORIZONTAL', { name: 'Mark', cornerRadius: 999, primaryAxisAlignItems: 'CENTER', counterAxisAlignItems: 'CENTER' });
  if (state === 'done') { mark.fills = [paint('primary')]; mark.appendChild(icon('checkmark', 12, 'on-primary')); }
  else if (state === 'current') { mark.fills = [paint('sunrise')]; mark.appendChild(text(String(n), { size: 11, lh: 14, weight: 'Bold', color: 'on-sunrise' })); }
  else { mark.strokes = [paint('line-control')]; mark.strokeWeight = 1; mark.strokeAlign = 'INSIDE'; mark.appendChild(text(String(n), { size: 11, lh: 14, weight: 'Bold', color: 'ink-muted' })); }
  fixed(mark, 18, 18);
  r.appendChild(mark);
  r.appendChild(text(label, { weight: state === 'current' ? 'Bold' : 'Medium', color: state === 'current' ? 'ink' : state === 'done' ? 'ink-secondary' : 'ink-muted' }));
  if (state === 'current') { r.fills = [paint('surface-raised')]; r.effects = SHADOW; }
  return r;
}

// ---- window ---------------------------------------------------------------
const win = box('VERTICAL', { name: 'Onboarding · Rituals and routines', cornerRadius: 10, clipsContent: true });
win.fills = [paint('surface-window')];
fixed(win, 1100, 1040);
win.x = 1400; win.y = 100;
win.name = 'Onboarding · Rituals and routines (canvas 2026-10-18 13:15)';
figma.currentPage.appendChild(win);

const bar = box('HORIZONTAL', { name: 'Title bar', paddingLeft: 12, paddingRight: 12, itemSpacing: 8, counterAxisAlignItems: 'CENTER' });
bar.fills = [paint('surface-sunken')]; edge(bar, 'bottom');
const lights = box('HORIZONTAL', { name: 'Traffic lights', itemSpacing: 8, paddingRight: 12 });
for (const [n, c] of [['Close', 'traffic-close'], ['Minimize', 'traffic-minimize'], ['Zoom', 'traffic-zoom']]) {
  const e = figma.createEllipse(); e.name = n; e.resize(11, 11); e.fills = [paint(c)]; lights.appendChild(e);
}
bar.appendChild(lights);
bar.appendChild(spacer()); fill(bar.children[1]);
bar.appendChild(text('Set up Vaultday', { weight: 'SemiBold' }));
bar.appendChild(spacer()); fill(bar.children[3]);
const balance = spacer('Balance'); balance.resize(74, 1); bar.appendChild(balance);
height(bar, 38);
win.appendChild(bar); fill(bar); bar.layoutSizingVertical = 'FIXED';

const body = box('HORIZONTAL', { name: 'Body' });
win.appendChild(body); fill(body, true, true);

// Sidebar
const side = box('VERTICAL', { name: 'Setup steps', paddingTop: 24, paddingBottom: 24, paddingLeft: 16, paddingRight: 16, itemSpacing: 16 });
side.fills = [paint('surface-sidebar')]; edge(side, 'right');
side.resize(240, 100);
body.appendChild(side); side.layoutSizingHorizontal = 'FIXED'; side.layoutSizingVertical = 'FILL';

const brand = box('HORIZONTAL', { name: 'Brand', paddingLeft: 6, paddingRight: 6, itemSpacing: 8, counterAxisAlignItems: 'CENTER' });
const appIcon = box('HORIZONTAL', { name: 'App icon (placeholder)', cornerRadius: 8, primaryAxisAlignItems: 'CENTER', counterAxisAlignItems: 'CENTER' });
appIcon.fills = [paint('primary')];
const sun = figma.createEllipse(); sun.name = 'Sun'; sun.resize(14, 14); sun.fills = [paint('sunrise')]; appIcon.appendChild(sun);
fixed(appIcon, 34, 34);
brand.appendChild(appIcon);
const brandText = box('VERTICAL', { name: 'Brand text' });
brandText.appendChild(text('Vaultday', { weight: 'Bold' }));
brandText.appendChild(text('First run on this Mac', { size: 11, lh: 14, color: 'ink-muted' }));
brand.appendChild(brandText);
side.appendChild(brand); fill(brand);

const steps = box('VERTICAL', { name: 'Step list', itemSpacing: 2 });
const STEPS = ['Welcome', 'Workspaces', 'Folders', 'Sync and backup', 'Accounts', 'Feature sets', 'Security', 'Rituals and routines', 'Ready'];
STEPS.forEach((s, i) => { const n = i + 1; const r = stepRow(n, s, n < 8 ? 'done' : n === 8 ? 'current' : 'upcoming'); steps.appendChild(r); fill(r); });
side.appendChild(steps); fill(steps);

side.appendChild(spacer()); fill(side.children[2], false, true);

const scheme = box('HORIZONTAL', { name: 'Keyboard scheme', paddingLeft: 6, paddingRight: 6, itemSpacing: 6, counterAxisAlignItems: 'CENTER' });
const seg = segmented({ options: ['Standard', 'Vim motions'], selected: 'Vim motions', fill: true, ground: 'sunken' });
scheme.appendChild(seg); fill(seg);
scheme.appendChild(button({ icon: 'info.circle', size: 'sm' }));
side.appendChild(scheme); fill(scheme);

const hint = box('HORIZONTAL', { name: 'Hint', paddingLeft: 6, paddingRight: 6 });
hint.appendChild(text('These settings can be changed later in Settings', { size: 11, lh: 16, color: 'ink-muted', width: 200 }));
fill(hint.children[0]);
side.appendChild(hint); fill(hint);

// Main section
const main = box('VERTICAL', { name: 'Rituals and routines', paddingTop: 32, paddingLeft: 32, paddingRight: 32, itemSpacing: 12, clipsContent: true });
main.fills = [paint('surface')];
body.appendChild(main); fill(main, true, true);

const heading = box('VERTICAL', { name: 'Heading', itemSpacing: 6 });
heading.appendChild(text('Rituals and routines', { size: 20, lh: 26, weight: 'Bold', ls: -1, name: 'Title' }));
heading.appendChild(text('Work gets a planned day with a start and an end. Life gets routines, reminders that ring on time, and the evenings and weekends.', { lh: 19, color: 'ink-secondary', width: 640, name: 'Lead' }));
main.appendChild(heading); fill(heading);

const callout = box('HORIZONTAL', { name: 'Callout', paddingTop: 8, paddingBottom: 8, paddingLeft: 12, paddingRight: 12, itemSpacing: 8, cornerRadius: 12, counterAxisAlignItems: 'MIN' });
callout.fills = [paint('surface-sunken')];
callout.appendChild(icon('info.circle', 16, 'ink-muted'));
callout.appendChild(text('Tracked hours stay private on this Mac. They reach a time sink only for workspaces with Toggl and billing on', { size: 12, lh: 16, color: 'ink-secondary', width: 400 }));
fill(callout.children[1]);
main.appendChild(callout); fill(callout);

// Everywhere
const everywhere = panel('everywhere', 'Everywhere');
const grid3 = box('HORIZONTAL', { name: 'Cards', itemSpacing: 12 });
for (const o of [
  { icon: 'clock', title: 'Breaks', desc: 'Focus and break length', value: '50 · 10' },
  { icon: 'arrow.counterclockwise', title: 'Roll-over', desc: 'Where unfinished tasks land', value: 'To the top' },
  { icon: 'exclamationmark.triangle', title: 'Overrun', desc: 'Warn when a day runs over', value: 'Into free time' },
]) {
  const c = card({ icon: o.icon, title: o.title, desc: o.desc, trailing: select(o.value) });
  grid3.appendChild(c); fill(c);
}
everywhere.appendChild(grid3); fill(grid3);
main.appendChild(everywhere); fill(everywhere);

// Work + Life
const grid2 = box('HORIZONTAL', { name: 'Work and Life', itemSpacing: 12, counterAxisAlignItems: 'MIN' });
const work = panel('work', 'Work');
const hours = box('VERTICAL', { name: 'Hours and days', itemSpacing: 12 });
const range = box('HORIZONTAL', { name: 'Time range', itemSpacing: 6, counterAxisAlignItems: 'CENTER' });
range.appendChild(field('09:00', 58));
range.appendChild(text('–', { size: 12, lh: 16, color: 'ink-muted' }));
range.appendChild(field('17:30', 58));
hours.appendChild(range);
const days = box('HORIZONTAL', { name: 'Weekdays', itemSpacing: 8, counterAxisAlignItems: 'CENTER' });
for (const [d, on] of [['Mo', true], ['Tu', true], ['We', true], ['Th', true], ['Fr', true], ['Sa', false], ['Su', false]]) days.appendChild(checkbox(d, on));
hours.appendChild(days);
const workCards = [
  card({ icon: 'clock', tone: 'work', title: 'Working hours', desc: 'Flexible drops set hours: Vaultday plans around your calendar and a daily target instead', trailing: segmented({ options: ['Fixed', 'Flexible'], selected: 'Fixed' }), extra: hours }),
  card({ icon: 'sunrise', tone: 'work', title: 'Morning planning', desc: "Pick today's tasks against the calendar", trailing: field('08:45', 70) }),
  card({ icon: 'moon', tone: 'work', title: 'Shutdown', desc: 'Roll over what is left, write the day log', trailing: field('17:30', 70) }),
  card({ icon: 'checklist', tone: 'work', title: 'Weekly review', desc: 'What moved, what slipped', trailing: field('Fri 16:00', 80) }),
];
for (const c of workCards) { work.appendChild(c); fill(c); }
grid2.appendChild(work); fill(work);

const life = panel('life', 'Life');
const lifeCards = [
  card({ icon: 'moon', tone: 'life', title: 'Plan Life after work', desc: 'Life tasks go into evenings and weekends', trailing: toggle(true) }),
  card({ icon: 'repeat', tone: 'life', title: 'Routines', desc: 'Run · Mon Wed Fri 07:00 · Water the plants · every 3 days', trailing: button({ label: 'Add', size: 'sm' }) }),
  card({ icon: 'bell', tone: 'life', title: 'Reminder times ring', desc: 'Everything else waits for planning', trailing: toggle(true) }),
  card({ icon: 'sunrise', tone: 'life', title: 'Light planning', desc: "A few minutes after work to pick tonight's tasks", trailing: field('18:30', 80) }),
  card({ icon: 'folder', tone: 'life', title: 'Project review', desc: 'Next steps for each personal project', trailing: field('Sun 18:00', 80) }),
];
for (const c of lifeCards) { life.appendChild(c); fill(c); }
grid2.appendChild(life); fill(life);
main.appendChild(grid2); fill(grid2);

main.appendChild(spacer()); fill(main.children[main.children.length - 1], false, true);

// Footer
const footWrap = box('VERTICAL', { name: 'Footer', paddingTop: 16, paddingBottom: 16, itemSpacing: 12 });
footWrap.fills = [paint('surface')];
const foot = box('HORIZONTAL', { name: 'Setup footer', paddingTop: 16, paddingBottom: 16, itemSpacing: 8, counterAxisAlignItems: 'CENTER' });
edge(foot, 'top');
foot.appendChild(button({ icon: 'chevron.left', label: 'Back' }));
foot.appendChild(spacer()); fill(foot.children[1]);
const count = box('HORIZONTAL', { name: 'Step count', paddingRight: 4 });
count.appendChild(text('Step 8 of 9', { size: 11, lh: 14, color: 'ink-muted' }));
foot.appendChild(count);
foot.appendChild(button({ label: 'Continue', iconAfter: 'arrow.right', variant: 'primary' }));
footWrap.appendChild(foot); fill(foot);
main.appendChild(footWrap); fill(footWrap);

figma.viewport.scrollAndZoomIntoView([win]);
return { windowId: win.id, bounds: { w: win.width, h: win.height }, mainChildren: main.children.map(c => c.name + ' ' + Math.round(c.width) + 'x' + Math.round(c.height)), sideChildren: side.children.map(c => c.name + ' ' + Math.round(c.width) + 'x' + Math.round(c.height)), nodes: created.length };
