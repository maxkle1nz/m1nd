import { useState } from "react";
import { Link } from "wouter";
import { motion, AnimatePresence } from "framer-motion";
import { Button } from "@/components/ui/button";
import { M1ndWordmark } from "@/components/M1ndWordmark";

export function NavBar() {
  const [mobileOpen, setMobileOpen] = useState(false);
  const close = () => setMobileOpen(false);

  return (
    <header className="fixed top-0 left-0 right-0 z-50 bg-background/80 backdrop-blur-md border-b border-border/50">
      <div className="container mx-auto px-6 h-16 flex items-center justify-between">
        <Link href="/" onClick={close} className="flex items-center">
          <M1ndWordmark size={1.35} withGlyphs />
        </Link>

        <nav className="hidden md:flex items-center gap-8 text-sm font-medium text-muted-foreground">
          <a href="/#problem" className="hover:text-foreground transition-colors">The Problem</a>
          <a href="/#features" className="hover:text-foreground transition-colors">Features</a>
          <a href="/#workflow" className="hover:text-foreground transition-colors">Workflow</a>
          <Link href="/use-cases" className="hover:text-foreground transition-colors text-primary/80 hover:text-primary">
            Use Cases
          </Link>
          <Link href="/demo" className="hover:text-foreground transition-colors text-secondary/80 hover:text-secondary font-semibold">
            Live Demo
          </Link>
          <Link href="/l1ght" className="hover:text-foreground transition-colors font-semibold"
            style={{ color: "#ffb70099" }}
          >
            l1ght
          </Link>
          <a href="/#faq" className="hover:text-foreground transition-colors">FAQ</a>
        </nav>

        <div className="hidden md:flex items-center gap-4">
          <a href="https://m1nd.world/wiki/" target="_blank" rel="noreferrer">
            <Button variant="outline" className="border-primary/20 hover:bg-primary/10 text-primary">
              Documentation
            </Button>
          </a>
          <a href="https://github.com/maxkle1nz/m1nd" target="_blank" rel="noreferrer">
            <Button className="bg-primary text-primary-foreground hover:bg-primary/90">
              Get Started
            </Button>
          </a>
        </div>

        <button
          className="md:hidden flex flex-col items-center justify-center w-10 h-10 gap-1.5 rounded-md hover:bg-primary/10 transition-colors"
          onClick={() => setMobileOpen((v) => !v)}
          aria-label={mobileOpen ? "Close menu" : "Open menu"}
          aria-expanded={mobileOpen}
        >
          <motion.span
            className="block w-5 h-px bg-foreground origin-center"
            animate={mobileOpen ? { rotate: 45, y: 4 } : { rotate: 0, y: 0 }}
            transition={{ duration: 0.2 }}
          />
          <motion.span
            className="block w-5 h-px bg-foreground"
            animate={mobileOpen ? { opacity: 0, scaleX: 0 } : { opacity: 1, scaleX: 1 }}
            transition={{ duration: 0.2 }}
          />
          <motion.span
            className="block w-5 h-px bg-foreground origin-center"
            animate={mobileOpen ? { rotate: -45, y: -4 } : { rotate: 0, y: 0 }}
            transition={{ duration: 0.2 }}
          />
        </button>
      </div>

      <AnimatePresence>
        {mobileOpen && (
          <motion.div
            key="mobile-nav"
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: "auto" }}
            exit={{ opacity: 0, height: 0 }}
            transition={{ duration: 0.22, ease: "easeInOut" }}
            className="md:hidden overflow-hidden border-t border-border/30 bg-background/95 backdrop-blur-md"
          >
            <div className="container mx-auto px-6 py-5 flex flex-col gap-1">
              <a
                href="/#problem"
                onClick={close}
                className="py-3 text-sm font-medium text-muted-foreground hover:text-foreground border-b border-border/20 transition-colors"
              >
                The Problem
              </a>
              <a
                href="/#features"
                onClick={close}
                className="py-3 text-sm font-medium text-muted-foreground hover:text-foreground border-b border-border/20 transition-colors"
              >
                Features
              </a>
              <a
                href="/#workflow"
                onClick={close}
                className="py-3 text-sm font-medium text-muted-foreground hover:text-foreground border-b border-border/20 transition-colors"
              >
                Workflow
              </a>
              <Link
                href="/use-cases"
                onClick={close}
                className="py-3 text-sm font-medium text-primary/80 hover:text-primary border-b border-border/20 transition-colors"
              >
                Use Cases
              </Link>
              <Link
                href="/demo"
                onClick={close}
                className="py-3 text-sm font-semibold text-secondary/80 hover:text-secondary border-b border-border/20 transition-colors"
              >
                Live Demo
              </Link>
              <Link
                href="/l1ght"
                onClick={close}
                className="py-3 text-sm font-semibold border-b border-border/20 transition-colors"
                style={{ color: "#ffb70099" }}
              >
                l1ght
              </Link>
              <a
                href="/#faq"
                onClick={close}
                className="py-3 text-sm font-medium text-muted-foreground hover:text-foreground border-b border-border/20 transition-colors"
              >
                FAQ
              </a>
              <div className="flex flex-col gap-3 pt-4">
                <a href="https://m1nd.world/wiki/" target="_blank" rel="noreferrer" onClick={close}>
                  <Button variant="outline" className="w-full border-primary/20 hover:bg-primary/10 text-primary">
                    Documentation
                  </Button>
                </a>
                <a href="https://github.com/maxkle1nz/m1nd" target="_blank" rel="noreferrer" onClick={close}>
                  <Button className="w-full bg-primary text-primary-foreground hover:bg-primary/90">
                    Get Started
                  </Button>
                </a>
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </header>
  );
}
