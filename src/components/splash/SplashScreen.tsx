import React, { useEffect, useRef, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useUiStore } from '../../stores/uiStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { X } from 'lucide-react';

export const SplashScreen: React.FC = () => {
  const { setSplashDismissed } = useUiStore();
  const { settings } = useSettingsStore();

  const canvasRef = useRef<HTMLCanvasElement>(null);
  const flashRef = useRef<HTMLDivElement>(null);
  const logoRef = useRef<HTMLImageElement>(null);
  const sealRef = useRef<HTMLDivElement>(null);

  const [uiState, setUiState] = useState({
    logoLive: false,
    showBrand: false,
    showPresents: false,
    showDivider: false,
    showMessage: false,
    showButton: false,
  });

  const [canDismiss, setCanDismiss] = useState(false);
  const [isFadingOut, setIsFadingOut] = useState(false);
  const canDismissRef = useRef(false);
  const [initProgress, setInitProgress] = useState(15);
  const [initStepText, setInitStepText] = useState('Initializing BitTorrent core...');

  useEffect(() => {
    let cancelled = false;
    async function runSubsystemInit() {
      try {
        setInitProgress(25);
        setInitStepText('Checking librqbit engine status...');
        await invoke('get_engine_init_error');

        if (cancelled) return;
        setInitProgress(50);
        setInitStepText('Loading SQLite database & settings...');
        await invoke('get_user_settings');

        if (cancelled) return;
        setInitProgress(75);
        setInitStepText('Enumerating network & VPN adapters...');
        await invoke('get_network_interfaces');

        if (cancelled) return;
        setInitProgress(100);
        setInitStepText('Nebulae Subsystems Ready');
      } catch {
        setInitProgress(100);
        setInitStepText('Subsystems ready');
      }
    }
    runSubsystemInit();
    return () => {
      cancelled = true;
    };
  }, []);

  const handleDismiss = useCallback(() => {
    if (!canDismissRef.current) return;
    setIsFadingOut(true);
    setTimeout(() => {
      setSplashDismissed(true);
    }, 400);
  }, [setSplashDismissed]);

  const handleOpenCommunity = (e: React.MouseEvent) => {
    e.stopPropagation();
    const url = settings.splash_url || 'https://asiandvdclub.org';
    invoke('open_external_link', { url }).catch(() => {
      window.open(url, '_blank');
    });
  };

  // Keyboard dismiss (Escape, Space, Enter) - only active after 3 seconds
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' || e.key === ' ' || e.key === 'Enter') {
        if (!canDismissRef.current) return;
        e.preventDefault();
        handleDismiss();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleDismiss]);

  // UI entrance timeline matching Asian DVD Club streaming intro (watch.php)
  // Guaranteed to play for at least 3 seconds so users can read all text
  useEffect(() => {
    if (settings.enable_splash === false) {
      setSplashDismissed(true);
      return;
    }

    // Minimum display guarantee of at least 3 seconds (3000ms)
    const tMinDuration = setTimeout(() => {
      canDismissRef.current = true;
      setCanDismiss(true);
    }, 3000);

    // Entrance timeline: fast and elegant animation so everything is readable within the 3s window
    const tLogo = setTimeout(() => {
      setUiState((prev) => ({ ...prev, logoLive: true }));
    }, 200);

    const tFlash = setTimeout(() => {
      if (flashRef.current) {
        flashRef.current.style.opacity = '1';
        setTimeout(() => {
          if (flashRef.current) {
            flashRef.current.style.transition = 'opacity 0.6s ease';
            flashRef.current.style.opacity = '0';
          }
        }, 80);
      }
    }, 350);

    const tBrand = setTimeout(() => {
      setUiState((prev) => ({ ...prev, showBrand: true }));
    }, 500);

    const tPresents = setTimeout(() => {
      setUiState((prev) => ({ ...prev, showPresents: true }));
    }, 700);

    const tDivider = setTimeout(() => {
      setUiState((prev) => ({ ...prev, showDivider: true }));
    }, 900);

    const tMessage = setTimeout(() => {
      setUiState((prev) => ({ ...prev, showMessage: true }));
    }, 1100);

    const tButton = setTimeout(() => {
      setUiState((prev) => ({ ...prev, showButton: true }));
    }, 1500);

    // Auto-dismiss after users have had ample time (6 seconds total) to see and read what's written
    const tAutoDismiss = setTimeout(() => {
      canDismissRef.current = true;
      setCanDismiss(true);
      handleDismiss();
    }, 6000);

    return () => {
      clearTimeout(tMinDuration);
      clearTimeout(tLogo);
      clearTimeout(tFlash);
      clearTimeout(tBrand);
      clearTimeout(tPresents);
      clearTimeout(tDivider);
      clearTimeout(tMessage);
      clearTimeout(tButton);
      clearTimeout(tAutoDismiss);
    };
  }, [settings.enable_splash, handleDismiss, setSplashDismissed]);

  // Canvas animation logic replicating ADC streaming intro
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    let animId: number;
    let W = 0;
    let H = 0;
    let dpr = window.devicePixelRatio || 1;

    const resize = () => {
      dpr = window.devicePixelRatio || 1;
      W = window.innerWidth;
      H = window.innerHeight;
      canvas.width = W * dpr;
      canvas.height = H * dpr;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    };
    resize();
    window.addEventListener('resize', resize);

    const T0 = performance.now();
    const INTRO_DUR = 4200;
    const LOOP = 14000;

    // Sakura petals array
    const petals: Array<{
      x: number;
      y: number;
      s: number;
      rot: number;
      vx: number;
      vy: number;
      vr: number;
      wobbleA: number;
      wobbleF: number;
      wobbleP: number;
      alpha: number;
      depth: number;
      color: string;
    }> = [];
    for (let i = 0; i < 100; i++) {
      const hue = 340 + Math.random() * 30;
      const sat = 40 + Math.random() * 35;
      const lit = 70 + Math.random() * 20;
      petals.push({
        x: Math.random() * 2000 - 200,
        y: Math.random() * 1400 - 200,
        s: Math.random() * 7 + 3,
        rot: Math.random() * Math.PI * 2,
        vx: (Math.random() - 0.3) * 0.4,
        vy: Math.random() * 0.5 + 0.15,
        vr: (Math.random() - 0.5) * 0.02,
        wobbleA: Math.random() * 30 + 10,
        wobbleF: Math.random() * 0.003 + 0.001,
        wobbleP: Math.random() * Math.PI * 2,
        alpha: Math.random() * 0.5 + 0.2,
        depth: Math.random(),
        color: `hsla(${hue},${sat}%,${lit}%,`,
      });
    }

    const drawPetal = (x: number, y: number, s: number, rot: number, color: string, a: number) => {
      ctx.save();
      ctx.translate(x, y);
      ctx.rotate(rot);
      ctx.globalAlpha = a;
      ctx.fillStyle = color + a + ')';
      ctx.beginPath();
      ctx.moveTo(0, 0);
      ctx.bezierCurveTo(s * 0.5, -s * 0.8, s * 1.1, -s * 0.4, s * 0.9, 0);
      ctx.bezierCurveTo(s * 1.1, s * 0.4, s * 0.5, s * 0.8, 0, 0);
      ctx.fill();
      ctx.globalAlpha = a * 0.3;
      ctx.fillStyle = 'rgba(255,255,255,0.4)';
      ctx.beginPath();
      ctx.moveTo(s * 0.15, -s * 0.05);
      ctx.bezierCurveTo(s * 0.3, -s * 0.3, s * 0.65, -s * 0.15, s * 0.5, 0);
      ctx.bezierCurveTo(s * 0.35, s * 0.05, s * 0.2, 0.02, s * 0.15, -s * 0.05);
      ctx.fill();
      ctx.restore();
    };

    // Star sparkles array
    const sparkles: Array<{
      ang: number;
      dist: number;
      s: number;
      phase: number;
      speed: number;
      brightness: number;
    }> = [];
    for (let i = 0; i < 45; i++) {
      sparkles.push({
        ang: Math.random() * Math.PI * 2,
        dist: Math.random() * 180 + 40,
        s: Math.random() * 2.5 + 0.5,
        phase: Math.random() * Math.PI * 2,
        speed: Math.random() * 3 + 1.5,
        brightness: Math.random() * 0.6 + 0.2,
      });
    }

    const drawSparkle = (
      cx: number,
      cy: number,
      sp: (typeof sparkles)[0],
      t: number
    ) => {
      const a = sp.ang;
      const d = sp.dist;
      const x = cx + Math.cos(a) * d;
      const y = cy + Math.sin(a) * d;
      const twinkle = Math.pow(Math.max(0, Math.sin(t * sp.speed + sp.phase)), 3) * sp.brightness;
      if (twinkle < 0.02) return;
      ctx.save();
      ctx.globalAlpha = twinkle;
      ctx.translate(x, y);
      const sz = sp.s * (0.5 + twinkle);
      ctx.fillStyle = 'rgba(255,240,200,0.9)';
      ctx.fillRect(-sz / 2, -0.5, sz, 1);
      ctx.fillRect(-0.5, -sz / 2, 1, sz);
      ctx.fillStyle = 'rgba(255,255,255,0.6)';
      ctx.beginPath();
      ctx.arc(0, 0, sz * 0.3, 0, Math.PI * 2);
      ctx.fill();
      ctx.restore();
    };

    // Mist layers array
    const mists: Array<{
      x: number;
      y: number;
      w: number;
      h: number;
      speed: number;
      alpha: number;
    }> = [];
    for (let i = 0; i < 8; i++) {
      mists.push({
        x: Math.random() * 2000 - 400,
        y: Math.random() * 800 + 100,
        w: Math.random() * 400 + 200,
        h: Math.random() * 60 + 30,
        speed: (Math.random() * 0.08 + 0.02) * (i % 2 ? 1 : -1),
        alpha: Math.random() * 0.06 + 0.02,
      });
    }

    const easeOutBack = (t: number) => {
      const c1 = 1.70158;
      const c3 = c1 + 1;
      return 1 + c3 * Math.pow(t - 1, 3) + c1 * Math.pow(t - 1, 2);
    };

    const frame = (now: number) => {
      const raw = now - T0;
      const introP = Math.min(1, raw / INTRO_DUR);
      const loopT = (raw % LOOP) / LOOP;
      let cx = W / 2;
      let cy = H / 2 - 20;
      if (sealRef.current) {
        const rect = sealRef.current.getBoundingClientRect();
        cx = rect.left + rect.width / 2;
        cy = rect.top + rect.height / 2;
      }

      ctx.clearRect(0, 0, W, H);

      // 1. Background radial gradient
      const bg = ctx.createRadialGradient(cx, cy, 0, cx, cy, Math.max(W, H) * 0.8);
      bg.addColorStop(0, '#1a0808');
      bg.addColorStop(0.4, '#100406');
      bg.addColorStop(1, '#060204');
      ctx.fillStyle = bg;
      ctx.fillRect(0, 0, W, H);

      // 2. Seigaiha (traditional wave) pattern
      ctx.save();
      ctx.globalAlpha = 0.02;
      const waveR = 20;
      const waveRows = Math.ceil(H / (waveR * 1.5)) + 2;
      const waveCols = Math.ceil(W / (waveR * 2)) + 2;
      for (let wr = 0; wr < waveRows; wr++) {
        for (let wc = 0; wc < waveCols; wc++) {
          const wx = wc * waveR * 2 + (wr % 2) * waveR;
          const wy = wr * waveR * 1.5;
          ctx.strokeStyle = 'rgba(212,175,55,0.5)';
          ctx.lineWidth = 0.5;
          for (let ring = 1; ring <= 3; ring++) {
            ctx.beginPath();
            ctx.arc(wx, wy, (waveR * ring) / 3, Math.PI, 0);
            ctx.stroke();
          }
        }
      }
      ctx.restore();

      // 3. Rotating Golden Light Rays
      const rayAlpha = introP < 0.5 ? 0 : Math.min(1, (introP - 0.5) * 3);
      if (rayAlpha > 0) {
        ctx.save();
        ctx.translate(cx, cy);
        ctx.rotate(loopT * Math.PI * 2 * 0.08);
        const rayCount = 36;
        for (let r = 0; r < rayCount; r++) {
          const ang = (r / rayCount) * Math.PI * 2;
          const isLong = r % 3 === 0;
          const len = isLong ? Math.max(W, H) * 0.6 : Math.max(W, H) * 0.35;
          const rw = isLong ? 6 : 3;
          const rAlpha =
            (isLong ? 0.06 : 0.03) *
            rayAlpha *
            (0.6 + 0.4 * Math.sin(loopT * Math.PI * 2 * 2 + r * 0.7));
          ctx.save();
          ctx.rotate(ang);
          ctx.globalAlpha = rAlpha;
          const rg = ctx.createLinearGradient(0, 0, 0, -len);
          rg.addColorStop(0, 'rgba(212,175,55,0.8)');
          rg.addColorStop(0.3, 'rgba(212,175,55,0.3)');
          rg.addColorStop(1, 'transparent');
          ctx.fillStyle = rg;
          ctx.beginPath();
          ctx.moveTo(-rw, 0);
          ctx.lineTo(-rw * 0.2, -len);
          ctx.lineTo(rw * 0.2, -len);
          ctx.lineTo(rw, 0);
          ctx.closePath();
          ctx.fill();
          ctx.restore();
        }
        ctx.restore();
      }

      // 4. Sun Disc (Lacquer red with woodblock texture)
      const sunR = 56;
      let sunScale = introP < 0.35 ? 0 : Math.min(1, (introP - 0.35) * 3);
      sunScale = easeOutBack(sunScale);
      if (sunScale > 0.01) {
        const pulse = 1 + Math.sin(loopT * Math.PI * 2) * 0.02;
        const sR = sunR * sunScale * pulse;

        // Outer glow
        const og = ctx.createRadialGradient(cx, cy, sR * 0.5, cx, cy, sR * 3);
        og.addColorStop(0, 'rgba(180,30,15,0.25)');
        og.addColorStop(0.4, 'rgba(180,30,15,0.08)');
        og.addColorStop(1, 'transparent');
        ctx.fillStyle = og;
        ctx.beginPath();
        ctx.arc(cx, cy, sR * 3, 0, Math.PI * 2);
        ctx.fill();

        // Disc
        const dg = ctx.createRadialGradient(cx, cy - sR * 0.15, 0, cx, cy, sR);
        dg.addColorStop(0, '#d42a20');
        dg.addColorStop(0.6, '#b81a12');
        dg.addColorStop(1, '#8a100a');
        ctx.fillStyle = dg;
        ctx.beginPath();
        ctx.arc(cx, cy, sR, 0, Math.PI * 2);
        ctx.fill();

        // Woodblock texture lines
        ctx.save();
        ctx.globalAlpha = 0.08 * sunScale;
        ctx.strokeStyle = 'rgba(255,220,200,0.5)';
        ctx.lineWidth = 0.5;
        for (let tl = -sR; tl < sR; tl += 3) {
          const tdx = Math.sqrt(Math.max(0, sR * sR - tl * tl));
          if (tdx > 1) {
            ctx.beginPath();
            ctx.moveTo(cx - tdx, cy + tl);
            ctx.lineTo(cx + tdx, cy + tl);
            ctx.stroke();
          }
        }
        ctx.restore();
      }

      // 5. Ornamental Golden Ring with Tomoe and Cardinal Diamonds
      const ringR = 130;
      const ringProgress = introP < 0.15 ? 0 : Math.min(1, (introP - 0.15) * 2.5);
      if (ringProgress > 0) {
        const ringPulse = 1 + Math.sin(loopT * Math.PI * 2) * 0.003;
        const rR = ringR * ringPulse;
        const arcEnd = ringProgress * Math.PI * 2;

        ctx.save();
        // Main ring
        ctx.strokeStyle = 'rgba(212,175,55,0.5)';
        ctx.lineWidth = 1.5;
        ctx.beginPath();
        ctx.arc(cx, cy, rR, 0, arcEnd);
        ctx.stroke();

        // Inner ring
        ctx.strokeStyle = 'rgba(212,175,55,0.2)';
        ctx.lineWidth = 0.5;
        ctx.beginPath();
        ctx.arc(cx, cy, rR - 10, 0, arcEnd);
        ctx.stroke();

        // Outer dashed ring
        ctx.setLineDash([3, 7]);
        ctx.strokeStyle = 'rgba(212,175,55,0.15)';
        ctx.lineWidth = 0.5;
        ctx.beginPath();
        ctx.arc(cx, cy, rR + 10, 0, arcEnd);
        ctx.stroke();
        ctx.setLineDash([]);

        // Second outer ring
        ctx.strokeStyle = 'rgba(212,175,55,0.08)';
        ctx.lineWidth = 0.3;
        ctx.beginPath();
        ctx.arc(cx, cy, rR + 18, 0, arcEnd);
        ctx.stroke();
        ctx.restore();

        // Decorative elements along ring
        const decCount = 24;
        const ringRot = loopT * Math.PI * 2 * 0.03;
        for (let d = 0; d < decCount; d++) {
          const dProgress = d / decCount;
          if (dProgress > ringProgress) break;
          const dAng = (d / decCount) * Math.PI * 2 + ringRot;
          const dx = cx + Math.cos(dAng) * rR;
          const dy = cy + Math.sin(dAng) * rR;
          const twk = 0.4 + 0.3 * Math.sin(loopT * Math.PI * 4 + d * 1.3);
          ctx.save();
          ctx.globalAlpha = twk;
          if (d % 6 === 0) {
            ctx.translate(dx, dy);
            ctx.rotate(dAng + Math.PI / 4);
            ctx.fillStyle = 'rgba(212,175,55,0.7)';
            ctx.fillRect(-3.5, -3.5, 7, 7);
            ctx.restore();
          } else if (d % 3 === 0) {
            ctx.fillStyle = 'rgba(212,175,55,0.5)';
            ctx.beginPath();
            ctx.arc(dx, dy, 2.5, 0, Math.PI * 2);
            ctx.fill();
            ctx.restore();
          } else {
            ctx.fillStyle = 'rgba(212,175,55,0.35)';
            ctx.beginPath();
            ctx.arc(dx, dy, 1.2, 0, Math.PI * 2);
            ctx.fill();
            ctx.restore();
          }
        }

        // Tomoe (magatama)
        if (ringProgress > 0.8) {
          const tomoeAlpha = Math.min(1, (ringProgress - 0.8) * 5) * 0.3;
          for (let tm = 0; tm < 3; tm++) {
            const tAng = (tm / 3) * Math.PI * 2 + ringRot * 2 - Math.PI / 2;
            const tx = cx + Math.cos(tAng) * (rR + 28);
            const ty = cy + Math.sin(tAng) * (rR + 28);
            ctx.save();
            ctx.translate(tx, ty);
            ctx.rotate(tAng + Math.PI / 2);
            ctx.globalAlpha = tomoeAlpha;
            ctx.fillStyle = 'rgba(212,175,55,0.6)';
            ctx.beginPath();
            ctx.arc(0, -3, 5, 0, Math.PI);
            ctx.bezierCurveTo(-5, 5, -2, 8, 0, 3);
            ctx.fill();
            ctx.beginPath();
            ctx.arc(0, -3, 1.5, 0, Math.PI * 2);
            ctx.fillStyle = 'rgba(6,2,4,0.8)';
            ctx.fill();
            ctx.restore();
          }
        }

        // Stylized clouds (kumo)
        if (ringProgress > 0.6) {
          const kumoAlpha = Math.min(1, (ringProgress - 0.6) * 2.5) * 0.15;
          for (let k = 0; k < 4; k++) {
            const kAng = (k / 4) * Math.PI * 2 + Math.PI / 4;
            const kx = cx + Math.cos(kAng) * (rR - 25);
            const ky = cy + Math.sin(kAng) * (rR - 25);
            ctx.save();
            ctx.translate(kx, ky);
            ctx.rotate(kAng);
            ctx.globalAlpha = kumoAlpha;
            ctx.fillStyle = 'rgba(212,175,55,0.5)';
            ctx.beginPath();
            ctx.arc(0, 0, 8, 0, Math.PI * 2);
            ctx.arc(7, -2, 5, 0, Math.PI * 2);
            ctx.arc(-6, -1, 6, 0, Math.PI * 2);
            ctx.fill();
            ctx.restore();
          }
        }

        // Shimmer sweep around ring
        const shimmerAng = loopT * Math.PI * 2 * 0.6;
        ctx.save();
        const shimmerGrad = ctx.createRadialGradient(
          cx + Math.cos(shimmerAng) * rR,
          cy + Math.sin(shimmerAng) * rR,
          0,
          cx + Math.cos(shimmerAng) * rR,
          cy + Math.sin(shimmerAng) * rR,
          30
        );
        shimmerGrad.addColorStop(0, 'rgba(255,250,220,0.2)');
        shimmerGrad.addColorStop(1, 'transparent');
        ctx.fillStyle = shimmerGrad;
        ctx.beginPath();
        ctx.arc(
          cx + Math.cos(shimmerAng) * rR,
          cy + Math.sin(shimmerAng) * rR,
          30,
          0,
          Math.PI * 2
        );
        ctx.fill();
        ctx.restore();
      }

      // 6. Mist Layers
      for (let mi = 0; mi < mists.length; mi++) {
        const m = mists[mi];
        let mx = m.x + m.speed * raw * 0.04;
        if (mx > W + 400) mx -= W + 800;
        if (mx < -400) mx += W + 800;
        ctx.save();
        ctx.globalAlpha = m.alpha * (0.7 + 0.3 * Math.sin(loopT * Math.PI * 2 + mi * 1.1));
        const mg = ctx.createRadialGradient(mx + m.w / 2, m.y, 0, mx + m.w / 2, m.y, m.w / 2);
        mg.addColorStop(0, 'rgba(140,100,80,0.3)');
        mg.addColorStop(1, 'transparent');
        ctx.fillStyle = mg;
        ctx.beginPath();
        ctx.ellipse(mx + m.w / 2, m.y, m.w / 2, m.h, 0, 0, Math.PI * 2);
        ctx.fill();
        ctx.restore();
      }

      // 7. Drifting Sakura Petals
      const petalAlpha = introP < 0.6 ? 0 : Math.min(1, (introP - 0.6) * 2.5);
      if (petalAlpha > 0) {
        for (let pi = 0; pi < petals.length; pi++) {
          const p = petals[pi];
          const px =
            ((p.x + p.vx * raw * 0.06 + Math.sin(raw * p.wobbleF + p.wobbleP) * p.wobbleA) % W +
              W) %
            W;
          const py = (((p.y + p.vy * raw * 0.06) % (H + 100)) + H + 100) % (H + 100) - 50;
          const pr = p.rot + p.vr * raw;
          const pa = p.alpha * petalAlpha * (0.5 + 0.5 * Math.sin(raw * 0.001 + p.wobbleP));
          const ds = p.s * (0.7 + p.depth * 0.6);
          drawPetal(px, py, ds, pr, p.color, pa);
        }
      }

      // 8. Twinkling Sparkles
      const sparkAlpha = introP < 0.55 ? 0 : Math.min(1, (introP - 0.55) * 3);
      if (sparkAlpha > 0) {
        ctx.save();
        ctx.globalAlpha *= sparkAlpha;
        for (let si = 0; si < sparkles.length; si++) {
          drawSparkle(cx, cy, sparkles[si], raw * 0.001);
        }
        ctx.restore();
      }

      // 9. Film Grain
      ctx.save();
      ctx.globalAlpha = 0.025;
      for (let g = 0; g < 80; g++) {
        ctx.fillStyle = Math.random() > 0.5 ? 'rgba(255,255,255,0.8)' : 'rgba(0,0,0,0.8)';
        ctx.fillRect(Math.random() * W, Math.random() * H, 1, 1);
      }
      ctx.restore();

      // 10. Vignette
      const vig = ctx.createRadialGradient(
        cx,
        cy,
        Math.min(W, H) * 0.25,
        cx,
        cy,
        Math.max(W, H) * 0.72
      );
      vig.addColorStop(0, 'transparent');
      vig.addColorStop(1, 'rgba(0,0,0,0.6)');
      ctx.fillStyle = vig;
      ctx.fillRect(0, 0, W, H);

      animId = requestAnimationFrame(frame);
    };

    animId = requestAnimationFrame(frame);

    return () => {
      cancelAnimationFrame(animId);
      window.removeEventListener('resize', resize);
    };
  }, []);

  return (
    <div
      onClick={handleDismiss}
      className={`fixed inset-0 z-50 overflow-hidden bg-[#0a0a0c] text-white select-none font-serif cursor-default transition-opacity duration-400 ${
        isFadingOut ? 'opacity-0 pointer-events-none' : 'opacity-100'
      }`}
      style={{ fontFamily: "'Cinzel', 'Trajan Pro', Georgia, serif" }}
    >
      {/* 1. Full-Window Animated Intro Canvas */}
      <canvas
        ref={canvasRef}
        className="absolute inset-0 w-full h-full pointer-events-none z-0"
      />

      {/* 2. Radial Flash Overlay */}
      <div
        ref={flashRef}
        className="absolute inset-0 w-full h-full pointer-events-none z-30 transition-opacity duration-300 opacity-0"
        style={{
          background:
            'radial-gradient(circle, rgba(255,240,200,0.9) 0%, rgba(255,200,100,0.4) 40%, transparent 70%)',
        }}
      />

      {/* 3. Top Right Skip Button */}
      <div className="absolute top-6 right-6 z-40">
        <button
          onClick={(e) => {
            e.stopPropagation();
            handleDismiss();
          }}
          disabled={!canDismiss}
          className={`flex items-center gap-2 px-3.5 py-1.5 rounded-full border text-xs text-[#ECE9D8] backdrop-blur-md transition shadow-lg group ${
            canDismiss
              ? 'bg-black/40 hover:bg-black/60 border-[#d4af37]/30 hover:border-[#d4af37]/60 cursor-pointer'
              : 'bg-black/20 border-[#d4af37]/10 opacity-60 cursor-not-allowed'
          }`}
          title={canDismiss ? 'Skip intro (Esc)' : 'Intro playing...'}
        >
          <span className="font-serif tracking-widest text-[11px] uppercase">
            {canDismiss ? 'Skip' : 'Intro'}
          </span>
          {canDismiss && (
            <span className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-black/60 text-[#d4af37] border border-[#d4af37]/30 group-hover:text-white transition">
              Esc
            </span>
          )}
          {canDismiss && (
            <X className="w-3.5 h-3.5 text-[#d4af37] group-hover:text-white transition" />
          )}
        </button>
      </div>

      {/* 4. Central Asian DVD Club Cinematic UI */}
      <div className="absolute inset-0 w-full h-full z-20 flex flex-col items-center justify-center pointer-events-none">
        {/* ADC Circular Seal & Breathing Logo */}
        <div ref={sealRef} className="relative w-[300px] h-[300px] mb-5">
          <img
            ref={logoRef}
            src="/adc-logo.png"
            alt="Asian DVD Club"
            id="adc-logo-img"
            className={`absolute top-1/2 left-1/2 w-[110px] h-[110px] rounded-full z-10 transition-all duration-1000 ${
              uiState.logoLive ? 'live' : 'opacity-0 scale-0'
            }`}
            style={{
              transform: uiState.logoLive
                ? 'translate(-50%, -50%) scale(1)'
                : 'translate(-50%, -50%) scale(0)',
              boxShadow: '0 0 25px rgba(212, 175, 55, 0.4)',
            }}
          />
        </div>

        {/* Brand: ASIAN DVD CLUB */}
        <div
          className={`text-center transition-all duration-1000 ${
            uiState.showBrand ? 'opacity-100 translate-y-0' : 'opacity-0 translate-y-3'
          }`}
        >
          <p
            className="m-0 font-bold uppercase tracking-[7px] text-[15px] md:text-[16px]"
            style={{ color: 'rgba(212, 175, 55, 0.85)' }}
          >
            ASIAN DVD CLUB
          </p>
        </div>

        {/* Subtitle Replaced with: JOIN OUR COMMUNITY / ASIANDVDCLUB.ORG */}
        <div
          onClick={handleOpenCommunity}
          className={`mt-2 text-center transition-all duration-1000 pointer-events-auto cursor-pointer group ${
            uiState.showPresents ? 'opacity-100 translate-y-0' : 'opacity-0 translate-y-2'
          }`}
        >
          <p
            className="m-0 font-normal uppercase tracking-[5px] text-[20px] md:text-[22px]"
            style={{ color: 'rgba(236, 233, 216, 0.85)' }}
          >
            JOIN OUR COMMUNITY
          </p>
          <p
            className="mt-1 font-semibold uppercase tracking-[5px] text-[15px] md:text-[17px] group-hover:scale-105 transition-transform"
            style={{ color: 'rgba(212, 175, 55, 0.95)' }}
          >
            ASIANDVDCLUB.ORG
          </p>
        </div>

        {/* Golden Diamond Divider */}
        <div
          className={`my-5 transition-opacity duration-1000 ${
            uiState.showDivider ? 'opacity-100' : 'opacity-0'
          }`}
        >
          <svg width="200" height="12" viewBox="0 0 200 12">
            <line x1="0" y1="6" x2="85" y2="6" stroke="rgba(212,175,55,0.3)" strokeWidth="0.5" />
            <polygon points="100,0 106,6 100,12 94,6" fill="rgba(212,175,55,0.4)" />
            <line x1="115" y1="6" x2="200" y2="6" stroke="rgba(212,175,55,0.3)" strokeWidth="0.5" />
          </svg>
        </div>

        {/* Poetic Message */}
        <div
          className={`text-center max-w-md px-6 transition-all duration-1000 ${
            uiState.showMessage ? 'opacity-100 translate-y-0' : 'opacity-0 translate-y-3'
          }`}
          style={{ fontFamily: "'Cormorant Garamond', Georgia, serif" }}
        >
          <p className="m-0 text-[18px] md:text-[20px] leading-[1.8] font-light">
            <span className="text-white">R-torrent</span>
            <span style={{ color: '#d4af37' }} className="font-normal">
              –ALL trackers
            </span>
          </p>
        </div>

        {/* Subsystem Initialization Progress Bar (spec §5) */}
        <div className="mt-5 w-64 max-w-xs space-y-1.5 pointer-events-auto">
          <div className="flex justify-between items-center text-[10px] font-mono text-[#d4af37]/80">
            <span className="truncate max-w-[190px]">{initStepText}</span>
            <span>{initProgress}%</span>
          </div>
          <div className="h-1.5 w-full bg-black/60 rounded-full border border-[#d4af37]/20 overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-[#d4af37] via-[#e50914] to-[#d4af37] transition-all duration-300"
              style={{ width: `${initProgress}%` }}
            />
          </div>
        </div>

        {/* Imperial Crimson Cinematic START Button */}
        <div
          className={`mt-6 pointer-events-auto transition-all duration-1000 ${
            uiState.showButton
              ? 'opacity-100 translate-y-0 scale-100'
              : 'opacity-0 translate-y-3 scale-95'
          }`}
        >
          <button
            onClick={(e) => {
              e.stopPropagation();
              handleDismiss();
            }}
            disabled={!canDismiss}
            className={`adc-cine-btn ${!canDismiss ? 'opacity-70 cursor-not-allowed' : ''}`}
          >
            <span className="adc-cine-glow" />
            <span className="adc-cine-shine" />
            <span className="adc-cine-text">START</span>
          </button>
        </div>
      </div>

      {/* Scoped CSS animations replicating watch.php exactly */}
      <style>{`
        @keyframes adcCinePulse {
          0%, 100% {
            box-shadow: 0 0 15px rgba(212, 175, 55, 0.15), 0 0 40px rgba(132, 0, 1, 0.2), inset 0 0 20px rgba(212, 175, 55, 0.05);
            transform: scale(1);
          }
          50% {
            box-shadow: 0 0 25px rgba(212, 175, 55, 0.25), 0 0 60px rgba(132, 0, 1, 0.3), inset 0 0 30px rgba(212, 175, 55, 0.08);
            transform: scale(1.02);
          }
        }

        @keyframes adcCineShine {
          0% { left: -120%; }
          100% { left: 180%; }
        }

        @keyframes adcLogoBreath {
          0%, 100% {
            filter: drop-shadow(0 0 20px rgba(212, 175, 55, 0.3)) drop-shadow(0 0 40px rgba(180, 30, 20, 0.2)) brightness(1);
          }
          50% {
            filter: drop-shadow(0 0 35px rgba(212, 175, 55, 0.5)) drop-shadow(0 0 60px rgba(180, 30, 20, 0.3)) brightness(1.06);
          }
        }

        .adc-cine-btn {
          position: relative;
          overflow: hidden;
          background: linear-gradient(135deg, #5a0000 0%, #840001 30%, #a01010 50%, #840001 70%, #5a0000 100%);
          color: #ECE9D8;
          font-family: 'Cinzel', serif;
          font-weight: 700;
          font-size: 14px;
          padding: 15px 46px;
          border-radius: 50px;
          border: 1px solid rgba(212, 175, 55, 0.3);
          cursor: pointer;
          letter-spacing: 4px;
          text-transform: uppercase;
          animation: adcCinePulse 4s ease-in-out infinite;
          transition: all 0.3s ease;
        }

        .adc-cine-btn:hover {
          border-color: rgba(212, 175, 55, 0.6);
          background: linear-gradient(135deg, #6a0505 0%, #a01010 30%, #c42020 50%, #a01010 70%, #6a0505 100%);
        }

        .adc-cine-glow {
          position: absolute;
          top: -2px;
          left: -2px;
          right: -2px;
          bottom: -2px;
          border-radius: 52px;
          background: transparent;
          box-shadow: 0 0 15px rgba(212, 175, 55, 0.08);
          pointer-events: none;
        }

        .adc-cine-shine {
          position: absolute;
          top: 0;
          left: -120%;
          width: 50%;
          height: 100%;
          background: linear-gradient(90deg, transparent, rgba(255, 255, 255, 0.08), rgba(255, 255, 255, 0.15), rgba(255, 255, 255, 0.08), transparent);
          animation: adcCineShine 5s ease-in-out infinite;
          pointer-events: none;
        }

        .adc-cine-text {
          position: relative;
          z-index: 1;
        }

        #adc-logo-img.live {
          transform: translate(-50%, -50%) scale(1) !important;
          opacity: 1 !important;
          animation: adcLogoBreath 5s ease-in-out infinite;
        }
      `}</style>
    </div>
  );
};

export default SplashScreen;
