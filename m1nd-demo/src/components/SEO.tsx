import { useEffect } from "react";

const BASE_URL = "https://m1nd.world";

interface SEOProps {
  title: string;
  description: string;
  canonicalPath?: string;
  ogImage?: string;
}

export function SEO({
  title,
  description,
  canonicalPath = "",
  ogImage = `${BASE_URL}/opengraph.jpg`,
}: SEOProps) {
  useEffect(() => {
    document.title = title;

    const setMeta = (nameOrProp: string, content: string, isProp = false) => {
      const attr = isProp ? "property" : "name";
      let el = document.querySelector(`meta[${attr}="${nameOrProp}"]`);
      if (!el) {
        el = document.createElement("meta");
        el.setAttribute(attr, nameOrProp);
        document.head.appendChild(el);
      }
      el.setAttribute("content", content);
    };

    const canonicalUrl = `${BASE_URL}${canonicalPath}`;

    setMeta("description", description);
    setMeta("og:title", title, true);
    setMeta("og:description", description, true);
    setMeta("og:type", "website", true);
    setMeta("og:url", canonicalUrl, true);
    setMeta("og:image", ogImage, true);
    setMeta("og:site_name", "m1nd", true);
    setMeta("twitter:card", "summary_large_image");
    setMeta("twitter:title", title);
    setMeta("twitter:description", description);
    setMeta("twitter:image", ogImage);

    let canonical = document.querySelector<HTMLLinkElement>("link[rel='canonical']");
    if (!canonical) {
      canonical = document.createElement("link");
      canonical.rel = "canonical";
      document.head.appendChild(canonical);
    }
    canonical.href = canonicalUrl;

    return () => {
      document.title = "m1nd — Graph Intelligence";
    };
  }, [title, description, canonicalPath, ogImage]);

  return null;
}
