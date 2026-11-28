<script>
  import { untrack } from 'svelte';

  let { pony } = $props();

  // Uniform 1:1 native pixel multiplier keeps every pony's body identical across GIF canvas sizes.
  const PIXEL_SCALE = 1.0;
  const FALLBACK_SIZE = Math.round(64 * PIXEL_SCALE);

  const MARGIN_X = 28;
  const MIN_Y = 76;
  const MARGIN_BOTTOM = 16;
  const WALK_SPEED = 68;
  const FLY_SPEED = 104;
  const DRAG_THRESHOLD_PX = 5;

  let animState = $state('stand');
  let posX = $state(180);
  let posY = $state(520);
  let hoverOffsetY = $state(0);
  let facing = $state(1);
  let isHopping = $state(false);
  let isDragging = $state(false);
  let particles = $state([]);
  let imgFailed = $state(false);
  let imgWidth = $state(FALLBACK_SIZE);
  let imgHeight = $state(FALLBACK_SIZE);

  let imgEl = $state(null);

  let targetX = 180;
  let targetY = 520;
  let particleSeq = 0;
  let stateTimer = null;
  let hopTimer = null;

  let dragActive = false;
  let dragMoved = false;
  let dragPointerId = null;
  let dragStartX = 0;
  let dragStartY = 0;
  let dragOffsetX = 0;
  let dragOffsetY = 0;

  let spriteSrc = $derived(
    imgFailed ? `/ponies/${pony.id}/stand.gif` : `/ponies/${pony.id}/${animState}.gif`
  );

  function getBounds() {
    if (typeof window === 'undefined') {
      return { minX: MARGIN_X, maxX: 900, minY: MIN_Y, maxY: 640 };
    }
    const maxX = Math.max(MARGIN_X, window.innerWidth - MARGIN_X);
    const maxY = Math.max(MIN_Y, window.innerHeight - MARGIN_BOTTOM);
    return { minX: MARGIN_X, maxX, minY: MIN_Y, maxY };
  }

  function clampPos(x, y) {
    const b = getBounds();
    return {
      x: Math.min(b.maxX, Math.max(b.minX, x)),
      y: Math.min(b.maxY, Math.max(b.minY, y)),
    };
  }

  function pickRandomTarget() {
    const b = getBounds();
    for (let attempt = 0; attempt < 5; attempt++) {
      const tx = b.minX + Math.random() * (b.maxX - b.minX);
      const ty = b.minY + Math.random() * (b.maxY - b.minY);
      if (Math.hypot(tx - posX, ty - posY) >= 96 || attempt === 4) {
        targetX = tx;
        targetY = ty;
        if (Math.abs(tx - posX) > 4) {
          facing = tx >= posX ? 1 : -1;
        }
        return;
      }
    }
  }

  function pickNextState(prev) {
    const roll = Math.random();
    if (roll < 0.48) return 'walk';
    if (roll < 0.70) return 'stand';
    if (roll < 0.88) return 'special';
    return prev === 'sleep' ? 'walk' : 'sleep';
  }

  function scheduleNextTransition(minMs = 4000, maxMs = 8000) {
    if (stateTimer) clearTimeout(stateTimer);
    const delay = minMs + Math.random() * (maxMs - minMs);
    stateTimer = setTimeout(() => {
      if (isDragging) return;
      const next = pickNextState(animState);
      imgFailed = false;
      animState = next;
      if (next === 'walk' || (next === 'special' && pony.canFly)) {
        pickRandomTarget();
      }
      scheduleNextTransition();
    }, delay);
  }

  function spawnClickParticles() {
    const glyphs = ['♥', '✦', '★', '♥', '✧', '♥'];
    const palette = [pony.accent || '#F472B6', '#F472B6', '#FACC15', '#C084FC', '#38BDF8'];
    const batch = Array.from({ length: 6 }, (_, i) => {
      particleSeq += 1;
      const spreadX = Math.round((i - 2.5) * 14 + (Math.random() - 0.5) * 10);
      return {
        id: particleSeq,
        glyph: glyphs[particleSeq % glyphs.length],
        color: palette[particleSeq % palette.length],
        hx: `${spreadX}px`,
        offsetX: Math.round((Math.random() - 0.5) * 28),
      };
    });

    particles = [...particles, ...batch];
    const batchIds = new Set(batch.map((p) => p.id));
    setTimeout(() => {
      particles = particles.filter((p) => !batchIds.has(p.id));
    }, 980);
  }

  function triggerClickReaction() {
    imgFailed = false;
    animState = 'special';
    if (pony.canFly) {
      pickRandomTarget();
    }
    spawnClickParticles();

    if (hopTimer) clearTimeout(hopTimer);
    isHopping = false;
    requestAnimationFrame(() => {
      isHopping = true;
    });
    hopTimer = setTimeout(() => {
      isHopping = false;
    }, 440);

    scheduleNextTransition(3800, 5600);
  }

  function syncImageDimensions(el) {
    if (!el) return;
    if (el.naturalWidth > 0 && el.naturalHeight > 0) {
      imgWidth = Math.round(el.naturalWidth * PIXEL_SCALE);
      imgHeight = Math.round(el.naturalHeight * PIXEL_SCALE);
    }
  }

  function handleImgLoad(e) {
    syncImageDimensions(e?.currentTarget || imgEl);
  }

  function handleImgError() {
    if (animState !== 'stand') {
      imgFailed = true;
    }
  }

  function handlePointerDown(e) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();

    try {
      e.currentTarget.setPointerCapture(e.pointerId);
    } catch {
      // Ignore synthetic events without active pointer capture support
    }

    dragActive = true;
    dragMoved = false;
    dragPointerId = e.pointerId;
    dragStartX = e.clientX;
    dragStartY = e.clientY;
    dragOffsetX = posX - e.clientX;
    dragOffsetY = posY - e.clientY;
  }

  function handlePointerMove(e) {
    if (!dragActive || e.pointerId !== dragPointerId) return;
    e.preventDefault();

    const movedDist = Math.hypot(e.clientX - dragStartX, e.clientY - dragStartY);
    if (!dragMoved && movedDist >= DRAG_THRESHOLD_PX) {
      dragMoved = true;
      isDragging = true;
      if (stateTimer) clearTimeout(stateTimer);
      imgFailed = false;
      animState = 'special';
    }

    if (dragMoved) {
      const next = clampPos(e.clientX + dragOffsetX, e.clientY + dragOffsetY);
      const deltaX = next.x - posX;
      if (Math.abs(deltaX) > 1.5) {
        facing = deltaX > 0 ? 1 : -1;
      }
      posX = next.x;
      posY = next.y;
    }
  }

  function finishPointerInteraction(e, cancelled = false) {
    if (!dragActive || e.pointerId !== dragPointerId) return;

    try {
      if (e.currentTarget.hasPointerCapture?.(e.pointerId)) {
        e.currentTarget.releasePointerCapture(e.pointerId);
      }
    } catch {
      // Ignore capture release errors
    }

    const wasDrag = dragMoved;
    dragActive = false;
    dragMoved = false;
    dragPointerId = null;
    isDragging = false;

    if (cancelled) {
      animState = 'stand';
      scheduleNextTransition(1800, 3200);
      return;
    }

    if (!wasDrag) {
      triggerClickReaction();
    } else {
      targetX = posX;
      targetY = posY;
      imgFailed = false;
      animState = 'stand';
      scheduleNextTransition(1600, 3000);
    }
  }

  function handlePointerUp(e) {
    finishPointerInteraction(e, false);
  }

  function handlePointerCancel(e) {
    finishPointerInteraction(e, true);
  }

  function handleKeyDown(e) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      triggerClickReaction();
    }
  }

  $effect(() => {
    const _src = spriteSrc;
    untrack(() => {
      if (imgEl && imgEl.complete && imgEl.naturalWidth > 0) {
        syncImageDimensions(imgEl);
      }
    });
  });

  $effect(() => {
    const activeId = pony?.id;
    if (!activeId) return;

    let rafId = 0;
    let handleResize = null;

    untrack(() => {
      imgFailed = false;
      animState = 'stand';
      hoverOffsetY = 0;

      const bounds = getBounds();
      const initial = clampPos(
        posX || Math.round(bounds.minX + (bounds.maxX - bounds.minX) * 0.28),
        posY || Math.round(bounds.maxY - 24)
      );
      posX = initial.x;
      posY = initial.y;
      pickRandomTarget();

      scheduleNextTransition(2200, 4200);

      handleResize = () => {
        const clamped = clampPos(posX, posY);
        posX = clamped.x;
        posY = clamped.y;
        const clampedTarget = clampPos(targetX, targetY);
        targetX = clampedTarget.x;
        targetY = clampedTarget.y;
        if (Math.hypot(targetX - posX, targetY - posY) < 40) {
          pickRandomTarget();
        }
      };
      window.addEventListener('resize', handleResize);

      let lastTime = performance.now();

      const tick = (now) => {
        const dt = Math.min(0.05, Math.max(0, (now - lastTime) / 1000));
        lastTime = now;

        if (isDragging) {
          hoverOffsetY += (-10 - hoverOffsetY) * Math.min(1, dt * 14);
        } else {
          const isFlyingSpecial = animState === 'special' && pony.canFly;
          if (animState === 'walk' || isFlyingSpecial) {
            const speed = isFlyingSpecial ? FLY_SPEED : WALK_SPEED;
            const dx = targetX - posX;
            const dy = targetY - posY;
            const dist = Math.hypot(dx, dy);
            const step = speed * dt;

            if (dist <= Math.max(5, step)) {
              const arrived = clampPos(targetX, targetY);
              posX = arrived.x;
              posY = arrived.y;
              if (Math.random() < 0.35 && animState === 'walk') {
                animState = 'stand';
                scheduleNextTransition(2500, 5000);
              } else {
                pickRandomTarget();
              }
            } else {
              const next = clampPos(posX + (dx / dist) * step, posY + (dy / dist) * step);
              posX = next.x;
              posY = next.y;
              if (Math.abs(dx) > 2) {
                facing = dx >= 0 ? 1 : -1;
              }
            }

            if (isFlyingSpecial) {
              hoverOffsetY = -14 + Math.sin(now * 0.006) * 7;
            } else if (hoverOffsetY !== 0) {
              hoverOffsetY *= 0.84;
              if (Math.abs(hoverOffsetY) < 0.4) hoverOffsetY = 0;
            }
          } else if (hoverOffsetY !== 0) {
            hoverOffsetY *= 0.84;
            if (Math.abs(hoverOffsetY) < 0.4) hoverOffsetY = 0;
          }
        }

        rafId = requestAnimationFrame(tick);
      };

      rafId = requestAnimationFrame(tick);
    });

    return () => {
      cancelAnimationFrame(rafId);
      if (handleResize) window.removeEventListener('resize', handleResize);
      if (stateTimer) clearTimeout(stateTimer);
      if (hopTimer) clearTimeout(hopTimer);
    };
  });
</script>

<div
  class="fixed inset-0 z-[45] pointer-events-none select-none overflow-hidden"
  aria-hidden="false"
>
  <!-- Subtle grounded hoof shadow anchored to (posX, posY) -->
  <div
    class="pointer-events-none"
    style="position: absolute; left: 0; top: 0; width: 54px; height: 12px; border-radius: 9999px; background: radial-gradient(ellipse at center, rgba(0, 0, 0, 0.34) 0%, rgba(0, 0, 0, 0) 72%); filter: blur(2px); transform: translate3d({posX - 27}px, {posY - 6}px, 0) scale({isDragging ? 0.78 : hoverOffsetY < -4 ? 0.82 : 1}); opacity: {isDragging ? 0.45 : 0.85}; will-change: transform;"
  ></div>

  <!-- Interactive pony anchor (bottom-center at posX, posY) -->
  <div
    role="button"
    tabindex="0"
    title={pony.nameEn || pony.name}
    aria-label={pony.nameEn || pony.name}
    onpointerdown={handlePointerDown}
    onpointermove={handlePointerMove}
    onpointerup={handlePointerUp}
    onpointercancel={handlePointerCancel}
    onkeydown={handleKeyDown}
    class="focus:outline-none"
    style="position: absolute; left: 0; top: 0; width: 1px; height: 1px; overflow: visible; transform: translate3d({posX}px, {posY + hoverOffsetY}px, 0); will-change: transform; pointer-events: auto; touch-action: none; cursor: {isDragging ? 'grabbing' : 'grab'};"
  >
    <div class="relative {isHopping ? 'pony-hop-active' : ''}" style="width: 1px; height: 1px; overflow: visible;">
      <img
        bind:this={imgEl}
        src={spriteSrc}
        alt={pony.nameEn || pony.name}
        draggable="false"
        onload={handleImgLoad}
        onerror={handleImgError}
        class="select-none"
        style="position: absolute; bottom: 0; left: 50%; width: {imgWidth}px; height: {imgHeight}px; max-width: none; max-height: none; image-rendering: pixelated; transform: translateX(-50%) scaleX({facing}); filter: drop-shadow(0 4px 8px rgba(0, 0, 0, 0.35));"
      />

      {#each particles as p (p.id)}
        <span
          class="pony-heart-particle absolute text-sm font-bold pointer-events-none select-none"
          style="left: {p.offsetX}px; bottom: {Math.max(58, imgHeight - 8)}px; --hx: {p.hx}; color: {p.color}; text-shadow: 0 0 8px {p.color}; image-rendering: pixelated;"
        >
          {p.glyph}
        </span>
      {/each}
    </div>
  </div>
</div>
