// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

// Animation building blocks (Motion, https://motion.dev). Pages are
// prerendered with the elements in their hidden state: `data-motion` lets the
// <noscript> style in +Head show them when JavaScript is off. Visitors who
// prefer reduced motion get fades without movement (MotionConfig in +Layout).

import { motion, type HTMLMotionProps, type Variants } from "motion/react";
import { useEffect, useState, type ReactNode } from "react";

const EASE = [0.22, 1, 0.36, 1] as const;

type Direction = "up" | "left" | "right" | "scale";

const OFFSETS: Record<Direction, Record<string, number>> = {
  up: { y: 24 },
  left: { x: -32 },
  right: { x: 32 },
  scale: { scale: 0.95 },
};

function variants(direction: Direction, duration = 0.6): Variants {
  return {
    hidden: { opacity: 0, ...OFFSETS[direction] },
    visible: { opacity: 1, x: 0, y: 0, scale: 1, transition: { duration, ease: EASE } },
  };
}

/** Elements start animating when this share of them is visible, once. */
const VIEWPORT = { once: true, amount: 0.2 } as const;

interface RevealProps extends HTMLMotionProps<"div"> {
  from?: Direction;
  delay?: number;
}

/** Fades its content in when it scrolls into view. */
export function Reveal({ from = "up", delay = 0, ...props }: RevealProps) {
  return (
    <motion.div
      data-motion=""
      variants={variants(from)}
      initial="hidden"
      whileInView="visible"
      viewport={VIEWPORT}
      transition={{ delay }}
      {...props}
    />
  );
}

const container = (stagger: number, delay: number): Variants => ({
  hidden: {},
  visible: { transition: { staggerChildren: stagger, delayChildren: delay } },
});

interface StaggerProps {
  stagger?: number;
  delay?: number;
  /** Animate on mount (above the fold) instead of when scrolled into view. */
  onMount?: boolean;
}

function trigger(onMount: boolean) {
  return onMount
    ? ({ initial: "hidden", animate: "visible" } as const)
    : ({ initial: "hidden", whileInView: "visible", viewport: VIEWPORT } as const);
}

/** A list whose items ({@link StaggerItem}) appear one after the other. */
export function StaggerList({
  stagger = 0.08,
  delay = 0,
  onMount = false,
  ...props
}: StaggerProps & HTMLMotionProps<"ul">) {
  return <motion.ul variants={container(stagger, delay)} {...trigger(onMount)} {...props} />;
}

/** A block whose children ({@link StaggerChild}) appear one after the other. */
export function StaggerGroup({
  stagger = 0.08,
  delay = 0,
  onMount = false,
  ...props
}: StaggerProps & HTMLMotionProps<"div">) {
  return <motion.div variants={container(stagger, delay)} {...trigger(onMount)} {...props} />;
}

/** Item of a {@link StaggerList}; `lift` raises it on hover. */
export function StaggerItem({ lift = false, ...props }: { lift?: boolean } & HTMLMotionProps<"li">) {
  return (
    <motion.li
      data-motion=""
      variants={variants("up", 0.5)}
      whileHover={lift ? { y: -6, transition: { duration: 0.2 } } : undefined}
      {...props}
    />
  );
}

/** Child of a {@link StaggerGroup}. */
export function StaggerChild({ from = "up", ...props }: { from?: Direction } & HTMLMotionProps<"div">) {
  return <motion.div data-motion="" variants={variants(from, 0.5)} {...props} />;
}

/** Two soft shapes drifting behind a hero section (decorative). */
export function HeroBackdrop() {
  const blob = "bg-primary/20 absolute size-72 rounded-full blur-3xl lg:size-96";
  return (
    <div aria-hidden="true" className="pointer-events-none absolute inset-0 -z-10 overflow-hidden">
      <motion.div
        className={`${blob} -top-24 -left-16`}
        animate={{ x: [0, 60, 0], y: [0, 30, 0] }}
        transition={{ duration: 18, repeat: Infinity, ease: "easeInOut" }}
      />
      <motion.div
        className={`${blob} bg-secondary/15 -right-16 -bottom-32`}
        animate={{ x: [0, -50, 0], y: [0, -40, 0] }}
        transition={{ duration: 22, repeat: Infinity, ease: "easeInOut" }}
      />
    </div>
  );
}

/** Set once the first page has mounted in the browser. */
let navigated = false;

/**
 * Fades a page in after client-side navigation (keyed by URL). The first page
 * shows at once: its sections run their own entrance animations.
 */
export function PageFade({ children }: { children: ReactNode }) {
  const [first] = useState(() => !navigated);
  useEffect(() => {
    navigated = true;
    // Tells the fallback in +Head that animations run.
    (window as { __tessStarted?: boolean }).__tessStarted = true;
  }, []);
  return (
    <motion.div initial={first ? false : { opacity: 0 }} animate={{ opacity: 1 }} transition={{ duration: 0.25 }}>
      {children}
    </motion.div>
  );
}
