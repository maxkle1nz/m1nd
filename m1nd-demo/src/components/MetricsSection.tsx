export function MetricsSection() {
  return (
    <section className="py-24 bg-card border-t border-border/50">
      <div className="container mx-auto px-6">
        <div className="grid grid-cols-1 md:grid-cols-3 gap-12 divide-y md:divide-y-0 md:divide-x divide-border/50 text-center">
          <div className="flex flex-col gap-2 p-4">
            <h3 className="text-4xl md:text-5xl font-bold text-primary font-mono tracking-tighter">Less</h3>
            <p className="text-xl font-medium text-foreground">Token Burn</p>
            <p className="text-sm text-muted-foreground">Stop reading thousands of lines to find one symbol.</p>
          </div>
          <div className="flex flex-col gap-2 p-4">
            <h3 className="text-4xl md:text-5xl font-bold text-secondary font-mono tracking-tighter">Faster</h3>
            <p className="text-xl font-medium text-foreground">Orientation</p>
            <p className="text-sm text-muted-foreground">Navigate by structure and intent, not by grepping raw text.</p>
          </div>
          <div className="flex flex-col gap-2 p-4">
            <h3 className="text-4xl md:text-5xl font-bold text-accent font-mono tracking-tighter">Narrower</h3>
            <p className="text-xl font-medium text-foreground">Scope</p>
            <p className="text-sm text-muted-foreground">Precise blast-radius means surgical edits with high confidence.</p>
          </div>
        </div>
      </div>
    </section>
  );
}
