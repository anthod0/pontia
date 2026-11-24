import { places as landmarks } from "./demo";

type Point = { x: number; y: number };
type Destination = Point & { index: number; el: HTMLButtonElement };
const clamp = (v: number, min: number, max: number) => Math.max(min, Math.min(max, v));
const quad = (a: Point, q: Point, b: Point, t: number) => ({
  x: (1 - t) ** 2 * a.x + 2 * (1 - t) * t * q.x + t * t * b.x,
  y: (1 - t) ** 2 * a.y + 2 * (1 - t) * t * q.y + t * t * b.y,
});
const ease = (t: number) => (t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2);

export function mountStage(
  section: HTMLElement,
  onPlace: (index: number) => void,
  onSettle: (index: number) => void,
  onVisibility: (visible: boolean) => void,
) {
  const stage = section.querySelector<HTMLElement>(".stage")!;
  const you = stage.querySelector<HTMLButtonElement>(".you")!;
  const ask = stage.querySelector<HTMLElement>(".ask")!;
  const hint = stage.querySelector<HTMLElement>(".hint")!;
  const cta = stage.querySelector<HTMLAnchorElement>(".cta")!;
  const device = you.querySelector<HTMLElement>(".device")!;
  const reducedQuery = matchMedia("(prefers-reduced-motion: reduce)");
  const reduced = () => reducedQuery.matches;
  const machines = Array.from(stage.querySelectorAll<HTMLElement>(".machine"), (el, i) => ({
    el,
    anchor: { x: 0, y: 0 },
    mid: null as Point | null,
    midV: { x: 0, y: 0 },
    phase: i / 3,
    rope: stage.querySelectorAll("path.rope")[i],
    down: stage.querySelectorAll(".packet.down")[i],
    up: stage.querySelectorAll(".packet.up")[i],
    lock: stage.querySelectorAll<HTMLElement>(".lock")[i],
    eyes: el.querySelector(".eyes")!,
    pulse: stage.querySelectorAll<SVGCircleElement>(".pulse")[i],
    sent: null as { start: number; arrive: () => void } | null,
  }));
  const places = Array.from(stage.querySelectorAll<HTMLButtonElement>(".place"), (el, index) => ({
    ...landmarks[index],
    el,
    index,
    width: 0,
    height: 0,
    x: 0,
    y: 0,
  }));
  const listeners = new AbortController();
  const options = { signal: listeners.signal };
  const START = { d: [0.43, 0.6], m: [0.5, 0.53] };
  let W = 0,
    H = 0,
    compact = false,
    trayBottom = 0;
  const pos = { x: 0, y: 0 },
    vel = { x: 0, y: 0 };
  let mode = "idle",
    tilt = 0;
  let target: (Point & { place?: Destination }) | null = null;
  let settled: Destination | null = null;

  function layout() {
    const prevW = W,
      prevH = H;
    W = stage.clientWidth;
    H = stage.clientHeight;
    compact = W < 640;
    const sr = stage.getBoundingClientRect();
    for (const m of machines) {
      const r = m.el.getBoundingClientRect();
      m.anchor = { x: r.left - sr.left + r.width / 2, y: r.bottom - sr.top };
      trayBottom = r.bottom - sr.top;
    }
    for (const p of places) {
      const [fx, fy] = compact ? p.narrow : p.wide;
      p.x = fx * W;
      p.y = fy * H;
      p.width = p.el.offsetWidth;
      p.height = p.el.offsetHeight;
      p.el.style.left = p.x + "px";
      p.el.style.top = p.y + "px";
    }
    if (!prevW) {
      const [sx, sy] = compact ? START.m : START.d;
      pos.x = sx * W;
      pos.y = sy * H;
    } else if (settled) {
      pos.x = settled.x;
      pos.y = settled.y;
    } else {
      pos.x = (pos.x / prevW) * W;
      pos.y = (pos.y / prevH) * H;
    }
    clampPos();
  }
  const bounds = () => ({
    x0: Math.max(44, you.offsetWidth / 2 + 2),
    x1: W - Math.max(44, you.offsetWidth / 2 + 2),
    y0: trayBottom + 44,
    y1: H - 56,
  });
  function clampPos() {
    const b = bounds();
    pos.x = clamp(pos.x, b.x0, b.x1);
    pos.y = clamp(pos.y, b.y0, b.y1);
  }

  const magnet = () => (compact ? 54 : 70);
  function nearestPlace(x: number, y: number, radius: number) {
    let best = null,
      bestD = radius;
    for (const p of places) {
      const d = Math.hypot(p.x - x, p.y - y);
      if (d < bestD) {
        best = p;
        bestD = d;
      }
    }
    return best;
  }

  // ---------- Movement ----------
  let interacted = false;
  function firstTouch() {
    if (interacted) return;
    interacted = true;
    hint.classList.add("gone");
  }

  function goTo(point: Point & { place?: Destination }) {
    firstTouch();
    const b = bounds();
    target = { x: clamp(point.x, b.x0, b.x1), y: clamp(point.y, b.y0, b.y1), place: point.place };
    settled = null;
    if (reduced()) {
      pos.x = target.x;
      pos.y = target.y;
      vel.x = vel.y = 0;
      settle();
      return;
    }
    mode = "spring";
    cta.inert = true;
  }
  function settle() {
    mode = "idle";
    vel.x = vel.y = 0;
    const p = target?.place || nearestPlace(pos.x, pos.y, 1);
    settled = p && Math.hypot(p.x - pos.x, p.y - pos.y) < 1 ? p : null;
    target = null;
    onSettle(here()?.index ?? -1);
  }
  // After a flick or a drop, a nearby place pulls you in.
  function afterRelease() {
    const p = nearestPlace(pos.x, pos.y, magnet() * 1.4);
    if (p) goTo({ x: p.x, y: p.y, place: p });
    else settle();
  }

  let drag: {
    id: number;
    dx: number;
    dy: number;
    samples: (Point & { t: number })[];
    moved: number;
  } | null = null;
  you.addEventListener(
    "pointerdown",
    (e) => {
      if (!e.isPrimary || e.button !== 0) return;
      firstTouch();
      you.setPointerCapture(e.pointerId);
      const sr = stage.getBoundingClientRect();
      drag = {
        id: e.pointerId,
        dx: e.clientX - sr.left - pos.x,
        dy: e.clientY - sr.top - pos.y,
        samples: [],
        moved: 0,
      };
      mode = "drag";
      cta.inert = true;
      settled = null;
      vel.x = vel.y = 0;
      you.classList.add("dragging");
    },
    options,
  );
  you.addEventListener(
    "pointermove",
    (e) => {
      if (!drag || e.pointerId !== drag.id) return;
      const sr = stage.getBoundingClientRect();
      const nx = e.clientX - sr.left - drag.dx,
        ny = e.clientY - sr.top - drag.dy;
      drag.moved += Math.hypot(nx - pos.x, ny - pos.y);
      pos.x = nx;
      pos.y = ny;
      clampPos();
      const t = performance.now();
      drag.samples.push({ t, x: pos.x, y: pos.y });
      while (drag.samples.length > 2 && t - drag.samples[0].t > 90) drag.samples.shift();
    },
    options,
  );
  function endDrag(e: PointerEvent) {
    if (!drag || e.pointerId !== drag.id) return;
    const s = drag.samples,
      last = s[s.length - 1],
      first = s[0];
    const moved = drag.moved;
    drag = null;
    you.classList.remove("dragging");
    if (moved < 4) {
      mode = "idle";
      settle();
      return;
    }
    // Dropping onto a place keeps you there, even if the hand was still moving a little.
    const drop = nearestPlace(pos.x, pos.y, magnet());
    if (!reduced() && first && last && last.t - first.t > 8 && performance.now() - last.t < 80) {
      const dt = (last.t - first.t) / 1000;
      vel.x = clamp((last.x - first.x) / dt, -2600, 2600);
      vel.y = clamp((last.y - first.y) / dt, -2600, 2600);
      const speed = Math.hypot(vel.x, vel.y);
      if (speed > 140 && !(drop && speed < 1100)) {
        mode = "glide";
        return;
      }
    }
    afterRelease();
  }
  you.addEventListener("pointerup", endDrag, options);
  you.addEventListener("pointercancel", endDrag, options);
  you.addEventListener("lostpointercapture", endDrag, options);

  const directions: Partial<Record<string, readonly [number, number]>> = {
    ArrowLeft: [-1, 0],
    ArrowRight: [1, 0],
    ArrowUp: [0, -1],
    ArrowDown: [0, 1],
  };
  // Arrow keys hop to the nearest place in that direction.
  you.addEventListener(
    "keydown",
    (e) => {
      const dir = directions[e.key];
      if (!dir) return;
      e.preventDefault();
      let best = null,
        bestScore = Infinity;
      for (const p of places) {
        if (p === settled) continue;
        const dx = p.x - pos.x,
          dy = p.y - pos.y,
          d = Math.hypot(dx, dy);
        const cos = (dx * dir[0] + dy * dir[1]) / d;
        if (cos < 0.3) continue;
        const score = d / (cos * cos);
        if (score < bestScore) {
          best = p;
          bestScore = score;
        }
      }
      if (best) goTo({ x: best.x, y: best.y, place: best });
    },
    options,
  );

  for (const p of places)
    p.el.addEventListener("click", () => goTo({ x: p.x, y: p.y, place: p }), options);

  // Tapping open field walks you there.
  let tap: (Point & { t: number }) | null = null;
  stage.addEventListener(
    "pointerdown",
    (e) => {
      if ((e.target as Element).closest("button, a, .ask")) return;
      tap = { x: e.clientX, y: e.clientY, t: performance.now() };
    },
    options,
  );
  stage.addEventListener(
    "pointerup",
    (e) => {
      if (!tap || (e.target as Element).closest("button, a, .ask")) {
        tap = null;
        return;
      }
      const moved = Math.hypot(e.clientX - tap.x, e.clientY - tap.y),
        quick = performance.now() - tap.t < 500;
      tap = null;
      if (moved > 8 || !quick) return;
      const sr = stage.getBoundingClientRect();
      const x = e.clientX - sr.left,
        y = e.clientY - sr.top;
      const p = nearestPlace(x, y, magnet());
      goTo(p ? { x: p.x, y: p.y, place: p } : { x, y });
    },
    options,
  );

  section.querySelector<HTMLButtonElement>(".slot")!.addEventListener(
    "click",
    () => {
      const i = here()?.index ?? -1;
      const next = places[(i + 1) % places.length];
      goTo({ x: next.x, y: next.y, place: next });
    },
    options,
  );

  const here = () => nearestPlace(pos.x, pos.y, magnet());
  let lastPlace: number | undefined;
  let prev = performance.now(),
    frame = 0,
    visible = false;
  let previousX = 0,
    previousY = 0;
  let ctaSide = "right",
    swing = 0,
    swingV = 0;
  const setCtaSide = (side: string) => {
    ctaSide = side;
    cta.classList.toggle("left", side === "left");
  };

  function step(now: number) {
    const dt = Math.min(0.033, (now - prev) / 1000);
    prev = now;
    const rm = reduced();
    const px = previousX,
      py = previousY;
    if (rm && mode === "glide") afterRelease();
    if (rm && mode === "spring" && target) {
      pos.x = target.x;
      pos.y = target.y;
      settle();
    }

    if (mode === "glide") {
      const f = Math.exp(-dt * 3.4);
      vel.x *= f;
      vel.y *= f;
      pos.x += vel.x * dt;
      pos.y += vel.y * dt;
      const b = bounds();
      if (pos.x < b.x0 || pos.x > b.x1) {
        pos.x = clamp(pos.x, b.x0, b.x1);
        vel.x *= -0.55;
      }
      if (pos.y < b.y0 || pos.y > b.y1) {
        pos.y = clamp(pos.y, b.y0, b.y1);
        vel.y *= -0.55;
      }
      const speed = Math.hypot(vel.x, vel.y);
      // A slowing glide gets caught by a place it passes over.
      if (speed < 90 || (speed < 520 && nearestPlace(pos.x, pos.y, magnet() * 0.8))) afterRelease();
    } else if (mode === "spring" && target) {
      const k = 240,
        c = 2 * Math.sqrt(k) * 0.78;
      vel.x += (k * (target.x - pos.x) - c * vel.x) * dt;
      vel.y += (k * (target.y - pos.y) - c * vel.y) * dt;
      pos.x += vel.x * dt;
      pos.y += vel.y * dt;
      if (Math.hypot(target.x - pos.x, target.y - pos.y) < 0.6 && Math.hypot(vel.x, vel.y) < 8) {
        pos.x = target.x;
        pos.y = target.y;
        settle();
      }
    }

    // The phone leans into the direction you move it.
    const vx = mode === "drag" ? (pos.x - px) / Math.max(dt, 0.001) : vel.x;
    const vy = mode === "drag" ? (pos.y - py) / Math.max(dt, 0.001) : vel.y;
    tilt += ((rm ? 0 : clamp(vx * 0.012, -16, 16)) - tilt) * Math.min(1, dt * 12);
    you.style.transform = `translate(${pos.x}px, ${pos.y}px) translate(-50%, -28px) rotate(${tilt.toFixed(2)}deg)`;
    stage.style.setProperty("--gx", pos.x + "px");
    stage.style.setProperty("--gy", pos.y + "px");

    // Get started rides beside what you hold. Near a stage edge it flips to the side with room.
    const cw = cta.offsetWidth,
      ch = cta.offsetHeight,
      reach = device.offsetWidth / 2 + 14;
    const ctaFits = (side: string) =>
      side === "right" ? pos.x + reach + cw <= W - 6 : pos.x - reach - cw >= 6;
    const cy = pos.y - 13 - ch / 2;
    const ctaX = (side: string) => (side === "right" ? pos.x + reach : pos.x - reach - cw);
    // Once you stop, it also keeps clear of the places next to you when the other side has room.
    const place = here();
    const covers = (side: string) => {
      const x = ctaX(side);
      return places.some((q) =>
        q === place
          ? false
          : x < q.x + q.width / 2 &&
            q.x - q.width / 2 < x + cw &&
            cy < q.y - 19 + q.height &&
            q.y - 19 < cy + ch,
      );
    };
    const otherSide = ctaSide === "right" ? "left" : "right";
    if (mode === "idle") {
      const flip =
        ctaFits(otherSide) && (!ctaFits(ctaSide) || (covers(ctaSide) && !covers(otherSide)));
      // Change sides only once the existing fade-out has finished.
      const waitingToFlip = flip && !rm && getComputedStyle(cta).opacity !== "0";
      if (flip && !waitingToFlip) setCtaSide(otherSide);
      if (cta.inert !== waitingToFlip) cta.inert = waitingToFlip;
    }
    // The bubble hangs from its tail, so it swings a little when you move it up or down.
    const swingTo = rm ? 0 : clamp((ctaSide === "right" ? -vy : vy) * 0.006, -8, 8);
    if (rm) {
      swing = 0;
      swingV = 0;
    } else {
      swingV += (160 * (swingTo - swing) - 11 * swingV) * dt;
      swing += swingV * dt;
    }
    cta.style.transform = `translate(${ctaX(ctaSide).toFixed(1)}px, ${cy.toFixed(1)}px) rotate(${swing.toFixed(2)}deg)`;

    // Each device holds a link to you. The middle of the link is a damped spring, so links sway as you move.
    const T = { x: pos.x, y: pos.y };
    for (const m of machines) {
      const A = m.anchor,
        d = Math.hypot(T.x - A.x, T.y - A.y);
      const ideal = { x: (A.x + T.x) / 2, y: (A.y + T.y) / 2 + Math.min(70, 16 + d * 0.12) };
      if (!m.mid || rm) {
        m.mid = { ...ideal };
        m.midV = { x: 0, y: 0 };
      } else {
        const k = 90,
          c = 9;
        m.midV.x += (k * (ideal.x - m.mid.x) - c * m.midV.x) * dt;
        m.midV.y += (k * (ideal.y - m.mid.y) - c * m.midV.y) * dt;
        m.mid.x += m.midV.x * dt;
        m.mid.y += m.midV.y * dt;
      }
      const Q = { x: 2 * m.mid.x - (A.x + T.x) / 2, y: 2 * m.mid.y - (A.y + T.y) / 2 };
      m.rope.setAttribute(
        "d",
        `M${A.x.toFixed(1)} ${A.y.toFixed(1)}Q${Q.x.toFixed(1)} ${Q.y.toFixed(1)} ${T.x.toFixed(1)} ${T.y.toFixed(1)}`,
      );
      m.lock.style.transform = `translate(${m.mid.x}px, ${m.mid.y}px)`;
      if (!rm) {
        const t1 = ease((now / 2600 + m.phase) % 1),
          t2 = ease((now / 2600 + m.phase + 0.5) % 1);
        const a = quad(A, Q, T, t1),
          b = quad(T, Q, A, t2);
        m.down.setAttribute("cx", String(a.x));
        m.down.setAttribute("cy", String(a.y));
        m.up.setAttribute("cx", String(b.x));
        m.up.setAttribute("cy", String(b.y));
      }
      // The Bot's eyes follow you.
      const ex = clamp((T.x - A.x) / 120, -1, 1) * 3.2,
        ey = clamp((T.y - A.y) / 160, -0.4, 1) * 3;
      m.eyes.setAttribute("transform", `translate(${ex.toFixed(2)} ${ey.toFixed(2)})`);
      if (m.sent) {
        const t = rm ? 1 : Math.min(1, (now - m.sent.start) / 650);
        const pt = quad(T, Q, A, ease(t));
        m.pulse.setAttribute("cx", String(pt.x));
        m.pulse.setAttribute("cy", String(pt.y));
        if (t === 1) {
          m.pulse.style.display = "none";
          const { arrive } = m.sent;
          m.sent = null;
          arrive();
        }
      }
    }
    // The question sits on what you're holding.
    let below = pos.y + 62;
    if (!ask.hidden) {
      const aw = ask.offsetWidth,
        ah = ask.offsetHeight;
      // Prefer above what you're holding, then beside it on the side away from Get started, then below.
      let ax = clamp(pos.x - aw / 2, 8, W - aw - 8),
        ay = pos.y - ah - 46;
      if (ay < trayBottom + 6) {
        const side = ctaSide === "right" ? pos.x - 52 - aw : pos.x + 52;
        if (side >= 8 && side + aw <= W - 8) {
          ax = side;
          ay = clamp(pos.y - ah / 2 - 4, trayBottom + 6, H - ah - 6);
        } else {
          ay = pos.y + 58;
          below = ay + ah + 8;
        }
      }
      if (ay + ah > H - 6) ay = H - ah - 6;
      ask.style.transform = `translate(${ax}px, ${ay}px)`;
    }
    // The first-visit hint sits under you, clear of Get started and the request.
    const hw = hint.offsetWidth;
    hint.style.transform = `translate(${clamp(pos.x - hw / 2, 8, W - hw - 8)}px, ${below}px)`;
    previousX = pos.x;
    previousY = pos.y;
    for (const p of places) {
      p.el.classList.toggle("here", p === place && mode === "idle");
      p.el.classList.toggle("near", p === place && mode !== "idle");
    }
    const index = place?.index ?? -1;
    if (index !== lastPlace) {
      lastPlace = index;
      onPlace(index);
    }
    if (visible) frame = requestAnimationFrame(step);
  }
  const resize = new ResizeObserver(() => {
    layout();
    if (!visible) step(performance.now());
  });
  layout();
  step(performance.now());
  resize.observe(stage);
  resize.observe(machines[0].el);
  for (const p of places) resize.observe(p.el);
  const observer = new IntersectionObserver(
    ([entry]) => {
      visible = entry.isIntersecting;
      cancelAnimationFrame(frame);
      if (visible) {
        prev = performance.now();
        frame = requestAnimationFrame(step);
      }
      onVisibility(visible);
    },
    { threshold: 0.1 },
  );
  observer.observe(stage);
  return {
    send(index: number, arrive: () => void) {
      firstTouch();
      if (reduced()) {
        arrive();
        return;
      }
      const machine = machines[index];
      machine.sent = { start: performance.now(), arrive };
      machine.pulse.style.display = "";
    },
    destroy() {
      listeners.abort();
      observer.disconnect();
      resize.disconnect();
      cancelAnimationFrame(frame);
    },
  };
}
