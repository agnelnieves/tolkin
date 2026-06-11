"use client";

import { useEffect, useRef } from "react";
import {
  AdditiveBlending,
  BufferAttribute,
  BufferGeometry,
  PerspectiveCamera,
  Points,
  PointsMaterial,
  Scene,
  WebGLRenderer,
} from "three";
import { gsap, MOTION_OK } from "./motion";

const COUNT = 14000;
const CAMERA_Z = 60;
const FOV = 60;
const DEPTH = 30;
const MAX_PARALLAX_PX = 20;
const DPR_CAP = 1.75;

// Warm faint gray for 85 percent of points, lime for the rest. Values are
// linear-ish RGB fed straight to additive blending on a near-black canvas,
// so brightness lives in the numbers themselves.
const GRAY: [number, number, number] = [0.3, 0.29, 0.26];
const LIME: [number, number, number] = [0.63, 0.81, 0.33];

// Full-bleed particle field behind the hero. Loaded via next/dynamic with
// ssr: false and initialized only after first paint, on requestIdleCallback
// (setTimeout fallback). The render loop is a gsap.ticker callback that is
// attached only while the hero intersects the viewport AND the document is
// visible; off-screen or hidden-tab costs zero frames. DPR is capped at
// 1.75, antialias off, no shadows, no post-processing. Everything (geometry,
// material, renderer, observers, listeners) is disposed on unmount. Under
// prefers-reduced-motion this component renders only the static gradient
// base; the canvas never mounts.
export default function HeroField() {
  const wrapRef = useRef<HTMLDivElement>(null);
  const hostRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const wrap = wrapRef.current;
    const host = hostRef.current;
    if (!wrap || !host) return;

    const mm = gsap.matchMedia();

    mm.add(MOTION_OK, () => {
      let disposed = false;
      let idleId = 0;
      let fallbackId: ReturnType<typeof setTimeout> | undefined;
      let teardown: (() => void) | null = null;

      const start = () => {
        if (disposed) return;

        let renderer: WebGLRenderer;
        try {
          renderer = new WebGLRenderer({
            antialias: false,
            alpha: true,
            powerPreference: "low-power",
          });
        } catch {
          return; // No WebGL: the gradient base stays, nothing else mounts.
        }

        const dpr = Math.min(window.devicePixelRatio || 1, DPR_CAP);
        renderer.setPixelRatio(dpr);
        renderer.setClearColor(0x000000, 0);

        const scene = new Scene();
        const camera = new PerspectiveCamera(FOV, 1, 1, 200);
        camera.position.z = CAMERA_Z;

        // World-space extents of the viewport at the z=0 plane, refreshed on
        // resize. Used to size the field and to map parallax pixels to units.
        const visibleHeight = 2 * Math.tan((FOV * Math.PI) / 360) * CAMERA_Z;
        let worldPerPixel = 0.1;

        const resize = () => {
          const w = wrap.clientWidth;
          const h = wrap.clientHeight;
          if (w === 0 || h === 0) return;
          renderer.setSize(w, h, false);
          camera.aspect = w / h;
          camera.updateProjectionMatrix();
          worldPerPixel = visibleHeight / h;
        };
        resize();

        const halfX = visibleHeight * (wrap.clientWidth / Math.max(wrap.clientHeight, 1)) * 0.75;
        const halfY = visibleHeight * 0.75;

        const positions = new Float32Array(COUNT * 3);
        const velocities = new Float32Array(COUNT * 3);
        const colors = new Float32Array(COUNT * 3);
        for (let i = 0; i < COUNT; i++) {
          const j = i * 3;
          positions[j] = (Math.random() * 2 - 1) * halfX;
          positions[j + 1] = (Math.random() * 2 - 1) * halfY;
          positions[j + 2] = (Math.random() * 2 - 1) * DEPTH;
          velocities[j] = (Math.random() * 2 - 1) * 0.6;
          velocities[j + 1] = (Math.random() * 2 - 1) * 0.6;
          velocities[j + 2] = (Math.random() * 2 - 1) * 0.25;
          const tone = Math.random() < 0.15 ? LIME : GRAY;
          const dim = 0.4 + Math.random() * 0.8;
          colors[j] = tone[0] * dim;
          colors[j + 1] = tone[1] * dim;
          colors[j + 2] = tone[2] * dim;
        }

        const geometry = new BufferGeometry();
        geometry.setAttribute("position", new BufferAttribute(positions, 3));
        geometry.setAttribute("color", new BufferAttribute(colors, 3));

        const material = new PointsMaterial({
          size: 1.5 * dpr,
          sizeAttenuation: false,
          vertexColors: true,
          blending: AdditiveBlending,
          transparent: true,
          opacity: 0.8,
          depthWrite: false,
        });

        const points = new Points(geometry, material);
        scene.add(points);

        const canvas = renderer.domElement;
        canvas.className = "h-full w-full";
        canvas.style.opacity = "0";
        host.appendChild(canvas);
        const fadeIn = gsap.to(canvas, {
          opacity: 1,
          duration: 1.4,
          ease: "power2.out",
          paused: true,
        });

        // Pointer parallax target in screen pixels, hover-grade pointers
        // only. Touch devices keep the field but never read the pointer.
        const target = { x: 0, y: 0 };
        const shift = { x: 0, y: 0 };
        const pointerMM = gsap.matchMedia();
        pointerMM.add("(pointer: fine)", () => {
          const onPointerMove = (e: PointerEvent) => {
            target.x = (e.clientX / window.innerWidth - 0.5) * 2 * MAX_PARALLAX_PX;
            target.y = (e.clientY / window.innerHeight - 0.5) * 2 * MAX_PARALLAX_PX;
          };
          window.addEventListener("pointermove", onPointerMove, { passive: true });
          return () => {
            window.removeEventListener("pointermove", onPointerMove);
            target.x = 0;
            target.y = 0;
          };
        });

        const pos = geometry.attributes.position as BufferAttribute;
        const render = (_time: number, deltaTime: number) => {
          const dt = Math.min(deltaTime, 64) / 1000;
          const arr = pos.array as Float32Array;
          for (let i = 0; i < COUNT; i++) {
            const j = i * 3;
            arr[j] += velocities[j] * dt;
            arr[j + 1] += velocities[j + 1] * dt;
            arr[j + 2] += velocities[j + 2] * dt;
            if (arr[j] > halfX) arr[j] = -halfX;
            else if (arr[j] < -halfX) arr[j] = halfX;
            if (arr[j + 1] > halfY) arr[j + 1] = -halfY;
            else if (arr[j + 1] < -halfY) arr[j + 1] = halfY;
            if (arr[j + 2] > DEPTH) arr[j + 2] = -DEPTH;
            else if (arr[j + 2] < -DEPTH) arr[j + 2] = DEPTH;
          }
          pos.needsUpdate = true;

          // Lerped 1:1 parallax, capped at MAX_PARALLAX_PX screen pixels.
          shift.x += (target.x - shift.x) * Math.min(1, dt * 4);
          shift.y += (target.y - shift.y) * Math.min(1, dt * 4);
          points.position.x = shift.x * worldPerPixel;
          points.position.y = -shift.y * worldPerPixel;

          renderer.render(scene, camera);
        };

        // The ticker callback only exists while the hero is on screen and
        // the tab is visible. Both gates flow through syncTicker.
        let inView = false;
        let ticking = false;
        const syncTicker = () => {
          const shouldTick = inView && document.visibilityState === "visible";
          if (shouldTick && !ticking) {
            gsap.ticker.add(render);
            ticking = true;
            if (fadeIn.progress() === 0) fadeIn.play();
          } else if (!shouldTick && ticking) {
            gsap.ticker.remove(render);
            ticking = false;
          }
        };

        const io = new IntersectionObserver(
          (entries) => {
            inView = entries.some((entry) => entry.isIntersecting);
            syncTicker();
          },
          { rootMargin: "80px" },
        );
        io.observe(wrap);

        const onVisibility = () => syncTicker();
        document.addEventListener("visibilitychange", onVisibility);

        const ro = new ResizeObserver(resize);
        ro.observe(wrap);

        teardown = () => {
          io.disconnect();
          ro.disconnect();
          document.removeEventListener("visibilitychange", onVisibility);
          pointerMM.revert();
          if (ticking) gsap.ticker.remove(render);
          ticking = false;
          fadeIn.kill();
          geometry.dispose();
          material.dispose();
          renderer.dispose();
          canvas.remove();
        };
      };

      if (typeof window.requestIdleCallback === "function") {
        idleId = window.requestIdleCallback(start, { timeout: 2500 });
      } else {
        fallbackId = setTimeout(start, 300);
      }

      return () => {
        disposed = true;
        if (idleId) window.cancelIdleCallback?.(idleId);
        if (fallbackId) clearTimeout(fallbackId);
        teardown?.();
        teardown = null;
      };
    });

    return () => mm.revert();
  }, []);

  return (
    <div
      ref={wrapRef}
      aria-hidden="true"
      className="pointer-events-none absolute inset-0 overflow-hidden"
    >
      <div className="hero-field-glow absolute inset-0" />
      <div ref={hostRef} className="absolute inset-0" />
      <div className="hero-field-vignette absolute inset-0" />
    </div>
  );
}
