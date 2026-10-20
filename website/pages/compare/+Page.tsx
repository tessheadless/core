// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useData } from "vike-react/useData";
import { linkRel } from "../../components/SiteLink";
import { HeroBackdrop, StaggerChild, StaggerGroup, StaggerItem, StaggerList } from "../../components/Motion";
import { formatDate } from "../../lib/format";
import type { Data } from "./+data";

export default function Page() {
  const { competitors } = useData<Data>();
  const asOf = competitors.map((c) => c.asOf).sort()[0];
  return (
    <>
      <section className="from-primary/10 to-base-100 relative isolate bg-gradient-to-b py-16">
        <HeroBackdrop />
        <StaggerGroup onMount className="mx-auto max-w-4xl px-4 text-center">
          <StaggerChild>
            <p className="text-primary mb-3 font-semibold tracking-wide uppercase">Compare</p>
          </StaggerChild>
          <StaggerChild>
            <h1 className="text-4xl font-extrabold tracking-tight text-balance lg:text-5xl">How Tess compares</h1>
          </StaggerChild>
          <StaggerChild>
            <p className="text-base-content/80 mt-6 text-lg">
              Every CMS makes trade-offs. These pages say where Tess fits, and where another product is the better
              choice. Each one explains how to move your content models to Tess.
            </p>
          </StaggerChild>
        </StaggerGroup>
      </section>
      <section className="mx-auto w-full max-w-6xl px-4 py-12">
        <StaggerList className="grid gap-6 md:grid-cols-2" stagger={0.1}>
          {competitors.map((c) => (
            <StaggerItem key={c.slug} lift className="card bg-base-200 hover:shadow-lg">
              <div className="card-body">
                <h2 className="card-title text-2xl">Tess vs {c.name}</h2>
                <p className="text-primary text-sm font-semibold">{c.kind}</p>
                <p className="text-base-content/80">{c.summary}</p>
                <table className="table-sm mt-2 table">
                  <caption className="sr-only">Highlights: Tess and {c.name}</caption>
                  <thead>
                    <tr>
                      <th scope="col">
                        <span className="sr-only">Criterion</span>
                      </th>
                      <th scope="col">Tess</th>
                      <th scope="col">{c.name}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {c.rows.slice(0, 3).map((r) => (
                      <tr key={r.criterion}>
                        <th scope="row">{r.criterion}</th>
                        <td>{r.tess}</td>
                        <td>{r.other}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                <div className="card-actions mt-4 items-center justify-between">
                  {c.migrationGuideUrl ? (
                    <a
                      href={c.migrationGuideUrl}
                      rel={linkRel(c.migrationGuideUrl)}
                      className="link link-primary text-sm font-semibold"
                    >
                      Migration guide: from {c.name} to Tess
                    </a>
                  ) : (
                    <a href={c.importExportUrl} rel={linkRel(c.importExportUrl)} className="link link-primary text-sm">
                      Moving from {c.name}: import &amp; export tools
                    </a>
                  )}
                  <a href={`/compare/${c.slug}`} className="btn btn-primary btn-sm">
                    Full comparison
                  </a>
                </div>
              </div>
            </StaggerItem>
          ))}
        </StaggerList>
        {asOf && (
          <p className="text-base-content/60 mt-10 text-center text-sm">
            Facts checked on {formatDate(asOf)} or later, from each vendor's public documentation. Products change:
            check their sites before deciding.
          </p>
        )}
      </section>
    </>
  );
}
