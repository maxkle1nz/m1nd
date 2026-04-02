import { ReactNode } from "react";
import { motion } from "framer-motion";

interface FeatureSectionProps {
  title: string;
  subtitle: string;
  description: string;
  children: ReactNode;
  align?: "left" | "right";
  id?: string;
}

export function FeatureSection({ title, subtitle, description, children, align = "left", id }: FeatureSectionProps) {
  return (
    <section id={id} className="relative w-full min-h-[80vh] py-24 flex items-center overflow-hidden border-t border-border/30">
      <div className="container px-6 mx-auto relative z-10">
        <div className={`flex flex-col ${align === "right" ? "md:flex-row-reverse" : "md:flex-row"} items-center gap-12 lg:gap-24`}>
          <div className="w-full md:w-1/2 flex flex-col justify-center">
            <motion.div
              initial={{ opacity: 0, y: 30 }}
              whileInView={{ opacity: 1, y: 0 }}
              viewport={{ once: true, margin: "-100px" }}
              transition={{ duration: 0.7 }}
            >
              <h3 className="text-primary font-mono text-sm tracking-widest uppercase mb-4">{subtitle}</h3>
              <h2 className="text-3xl md:text-5xl font-bold font-sans tracking-tight mb-6">{title}</h2>
              <p className="text-lg text-muted-foreground leading-relaxed">
                {description}
              </p>
            </motion.div>
          </div>
          
          <div className="w-full md:w-1/2 h-[400px] md:h-[600px] relative rounded-lg border border-border/50 bg-background/50 overflow-hidden backdrop-blur-sm">
            {children}
          </div>
        </div>
      </div>
    </section>
  );
}
