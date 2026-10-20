// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useData } from "vike-react/useData";
import { Markdown } from "../../../components/Markdown";
import { HeroBackdrop, Reveal, StaggerChild, StaggerGroup } from "../../../components/Motion";
import { formatDate } from "../../../lib/format";
import type { Data } from "./+data";

export default function Page() {
  const { competitor: c, others } = useData<Data>();
  return (
    <>
      <section className="from-primary/10 to-base-100 relative isolate bg-gradient-to-b py-16">
        <HeroBackdrop />
        <StaggerGroup onMount className="mx-auto max-w-4xl px-4">
          <nav aria-label="Breadcrumb" className="breadcrumbs text-sm">
            <ul>
              <li>
                <a href="/compare">Compare</a>
              </li>
              <li aria-current="page">Tess vs {c.name}</li>
            </ul>
          </nav>
          <StaggerChild>
            <h1 className="mt-2 text-4xl font-extrabold tracking-tight lg:text-5xl">Tess vs {c.name}</h1>
          </StaggerChild>
          <StaggerChild>
            <p className="text-primary mt-3 font-semibold">{c.kind}</p>
            <p className="text-base-content/80 mt-4 text-lg">{c.summary}</p>
          </StaggerChild>
        </StaggerGroup>
      </section>

      <Reveal className="mx-auto w-full max-w-5xl px-4 py-10">
        <h2 className="mb-4 text-2xl font-bold">Side by side</h2>
        <div className="border-base-300 overflow-x-auto rounded-box border">
          <table className="table-zebra table">
            <caption className="sr-only">Tess compared with {c.name}</caption>
            <thead>
              <tr>
                <th scope="col" className="w-1/5">
                  <span className="sr-only">Criterion</span>
                </th>
                <th scope="col">Tess</th>
                <th scope="col">{c.name}</th>
              </tr>
            </thead>
            <tbody>
              {c.rows.map((r) => (
                <tr key={r.criterion}>
                  <th scope="row">{r.criterion}</th>
                  <td>{r.tess}</td>
                  <td>{r.other}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </Reveal>

      <section className="mx-auto grid w-full max-w-5xl gap-6 px-4 py-6 md:grid-cols-2">
        <Reveal from="left" className="card bg-base-200">
          <div className="card-body">
            <h2 className="card-title">Choose {c.name} if…</h2>
            <Markdown className="prose-base">{c.chooseThem}</Markdown>
          </div>
        </Reveal>
        <Reveal from="right" delay={0.1} className="card bg-primary/10">
          <div className="card-body">
            <h2 className="card-title">Choose Tess if…</h2>
            <Markdown className="prose-base">{c.chooseTess}</Markdown>
          </div>
        </Reveal>
      </section>

      <section className="mx-auto w-full max-w-5xl px-4 py-10">
        <Reveal from="scale" className="card border-primary border-2">
          <div className="card-body">
            <h2 className="card-title text-2xl">Moving from {c.name} to Tess</h2>
            <Markdown className="prose-base">{c.migration}</Markdown>
            <div className="card-actions mt-2">
              {c.migrationGuideUrl && (
                <a href={c.migrationGuideUrl} className="btn btn-primary">
                  Step-by-step migration guide
                </a>
              )}
              <a href={c.importExportUrl} className={`btn ${c.migrationGuideUrl ? "btn-outline" : "btn-primary"}`}>
                Read the import &amp; export guide
              </a>
            </div>
          </div>
        </Reveal>
      </section>

      <section className="mx-auto w-full max-w-5xl px-4 pb-6">
        <p className="text-base-content/60 text-sm">
          Facts checked on {formatDate(c.asOf)} from {c.name}'s public documentation
          {c.website && (
            <>
              {" "}
              (
              <a href={c.website} className="link" rel="noopener">
                {c.website.replace(/^https?:\/\//, "")}
              </a>
              )
            </>
          )}
          . Products change quickly: check the vendor's site for the latest features and pricing.
        </p>
        {others.length > 0 && (
          <nav aria-label="Other comparisons" className="mt-8">
            <h2 className="mb-3 font-semibold">Other comparisons</h2>
            <ul className="flex flex-wrap gap-2">
              {others.map((o) => (
                <li key={o.slug}>
                  <a href={`/compare/${o.slug}`} className="btn btn-sm btn-outline">
                    Tess vs {o.name}
                  </a>
                </li>
              ))}
            </ul>
          </nav>
        )}
      </section>
    </>
  );
}
