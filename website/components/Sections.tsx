// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { Competitor, Feature, Section } from "../lib/content";
import { CodeBlock } from "./CodeBlock";
import { Icon } from "./Icon";
import { Markdown } from "./Markdown";
import { HeroBackdrop, Reveal, StaggerChild, StaggerGroup, StaggerItem, StaggerList } from "./Motion";
import { useTranslation } from "react-i18next";
import { A, ButtonLink } from "./SiteLink";

export interface SectionContext {
  features: Feature[];
  competitors: Pick<Competitor, "name" | "slug" | "kind" | "summary">[];
}

/** Renders the `sections` dynamic zone of a SitePage, one block per component. */
export function Sections({ sections, context }: { sections: Section[]; context: SectionContext }) {
  return (
    <>
      {sections.map((s, i) => (
        <SectionBlock key={i} section={s} context={context} first={i === 0} />
      ))}
    </>
  );
}

function Container({ children, className = "" }: { children: React.ReactNode; className?: string }) {
  return <div className={`mx-auto w-full max-w-6xl px-4 ${className}`}>{children}</div>;
}

function SectionBlock({ section: s, context, first }: { section: Section; context: SectionContext; first: boolean }) {
  const { t } = useTranslation();
  switch (s.__typename) {
    case "SiteHero": {
      const Title = first ? "h1" : "h2";
      return (
        <section className="from-primary/10 to-base-100 relative isolate bg-gradient-to-b py-16 lg:py-24">
          <HeroBackdrop />
          <Container className={s.code ? "grid items-center gap-10 lg:grid-cols-2" : "text-center"}>
            <StaggerGroup onMount stagger={0.1}>
              {s.eyebrow && (
                <StaggerChild>
                  <p className="text-primary mb-3 font-semibold tracking-wide uppercase">{s.eyebrow}</p>
                </StaggerChild>
              )}
              <StaggerChild>
                <Title className="text-4xl font-extrabold tracking-tight text-balance lg:text-5xl">{s.title}</Title>
              </StaggerChild>
              <StaggerChild>
                <Markdown className={`text-base-content/80 mt-6 ${s.code ? "" : "mx-auto max-w-3xl"}`}>
                  {s.subtitle}
                </Markdown>
              </StaggerChild>
              <StaggerChild className={`mt-8 flex flex-wrap gap-3 ${s.code ? "" : "justify-center"}`}>
                <ButtonLink link={s.primary} className="btn-primary btn-lg" />
                <ButtonLink link={s.secondary} className="btn-outline btn-lg" />
              </StaggerChild>
            </StaggerGroup>
            {s.code && (
              <StaggerGroup onMount delay={0.3}>
                <StaggerChild from="right">
                  <CodeBlock code={s.code} language={s.codeLanguage ?? "graphql"} />
                </StaggerChild>
              </StaggerGroup>
            )}
          </Container>
        </section>
      );
    }
    case "SiteFeatureGrid": {
      const features = context.features.filter((f) => !s.highlightedOnly || f.highlighted);
      return (
        <section className="py-16">
          <Container>
            <Reveal>
              <h2 className="text-center text-3xl font-bold text-balance">{s.title}</h2>
              <Markdown className="text-base-content/80 mx-auto mt-4 max-w-3xl text-center">{s.subtitle}</Markdown>
            </Reveal>
            <StaggerList className="mt-10 grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
              {features.map((f) => (
                <StaggerItem key={f.title} lift className="card bg-base-200 hover:shadow-lg">
                  <div className="card-body">
                    <span className="bg-primary/15 text-primary flex size-11 items-center justify-center rounded-lg">
                      <Icon name={f.icon} />
                    </span>
                    <h3 className="card-title mt-2">{f.title}</h3>
                    <p className="text-base-content/80">{f.summary}</p>
                  </div>
                </StaggerItem>
              ))}
            </StaggerList>
          </Container>
        </section>
      );
    }
    case "SiteRichText":
      return (
        <section className="py-12">
          <Container className="max-w-3xl">
            <Reveal>
              {s.title && <h2 className="mb-4 text-3xl font-bold">{s.title}</h2>}
              <Markdown>{s.body}</Markdown>
            </Reveal>
          </Container>
        </section>
      );
    case "SiteCodeSample":
      return (
        <section className="bg-base-200 py-16">
          <Container className="grid items-center gap-10 lg:grid-cols-2">
            <Reveal from="left">
              <h2 className="text-3xl font-bold text-balance">{s.title}</h2>
              <Markdown className="text-base-content/80 mt-4">{s.body}</Markdown>
            </Reveal>
            <Reveal from="right" delay={0.15}>
              <CodeBlock code={s.code} language={s.language} />
            </Reveal>
          </Container>
        </section>
      );
    case "SiteComparisonTeaser":
      return (
        <section className="py-16">
          <Container>
            <Reveal>
              <h2 className="text-center text-3xl font-bold">{s.title}</h2>
              <Markdown className="text-base-content/80 mx-auto mt-4 max-w-3xl text-center">{s.subtitle}</Markdown>
            </Reveal>
            <StaggerList className="mt-10 grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
              {context.competitors.map((c) => (
                <StaggerItem key={c.slug} lift>
                  <A
                    href={`/compare/${c.slug}`}
                    ariaLabel={t("compare.tessVsKind", { name: c.name, kind: c.kind })}
                    className="card bg-base-200 hover:bg-base-300 h-full transition-colors"
                  >
                    <div className="card-body">
                      <h3 className="card-title">{t("compare.tessVs", { name: c.name })}</h3>
                      <p className="text-base-content/70 text-sm">{c.kind}</p>
                    </div>
                  </A>
                </StaggerItem>
              ))}
            </StaggerList>
            <div className="mt-8 text-center">
              <A href="/compare" className="btn btn-outline">
                {t("compare.allComparisons")}
              </A>
            </div>
          </Container>
        </section>
      );
    case "SiteFaq":
      return (
        <section className="py-16">
          <Container className="max-w-3xl">
            <Reveal>
              <h2 className="mb-6 text-3xl font-bold">{s.title}</h2>
            </Reveal>
            <StaggerGroup className="join join-vertical w-full" stagger={0.06}>
              {s.items.map((item) => (
                <StaggerChild key={item.question} className="join-item border-base-300 border">
                  <details className="collapse-arrow collapse">
                    <summary className="collapse-title text-lg font-semibold">{item.question}</summary>
                    <div className="collapse-content">
                      <Markdown>{item.answer}</Markdown>
                    </div>
                  </details>
                </StaggerChild>
              ))}
            </StaggerGroup>
          </Container>
        </section>
      );
    case "SiteCallToAction":
      return (
        <section className="py-16">
          <Container>
            <Reveal from="scale" className="bg-primary text-primary-content rounded-box px-6 py-12 text-center">
              <h2 className="text-3xl font-bold text-balance">{s.title}</h2>
              <Markdown className="prose-invert mx-auto mt-4 max-w-2xl">{s.body}</Markdown>
              <div className="mt-8 flex flex-wrap justify-center gap-3">
                <ButtonLink link={s.primary} className="btn-accent btn-lg" />
                <ButtonLink link={s.secondary} className="btn-ghost btn-lg" />
              </div>
            </Reveal>
          </Container>
        </section>
      );
    default:
      // A section type added in Tess but not yet supported by the website.
      return null;
  }
}
