// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useData } from "vike-react/useData";
import { formatDate } from "../../lib/format";
import type { Data } from "./+data";

export default function Page() {
  const { competitors } = useData<Data>();
  const asOf = competitors.map((c) => c.asOf).sort()[0];
  return (
    <>
      <section className="from-primary/10 to-base-100 bg-gradient-to-b py-16">
        <div className="mx-auto max-w-4xl px-4 text-center">
          <p className="text-primary mb-3 font-semibold tracking-wide uppercase">Compare</p>
          <h1 className="text-4xl font-extrabold tracking-tight text-balance lg:text-5xl">How Tess compares</h1>
          <p className="text-base-content/80 mt-6 text-lg">
            Every CMS makes trade-offs. These pages say where Tess fits, and where another product is the better choice.
            Each one explains how to move your content models to Tess.
          </p>
        </div>
      </section>
      <section className="mx-auto w-full max-w-6xl px-4 py-12">
        <ul className="grid gap-6 md:grid-cols-2">
          {competitors.map((c) => (
            <li key={c.slug} className="card bg-base-200">
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
                  <a href={c.importExportUrl} className="link link-primary text-sm">
                    Moving from {c.name}: import &amp; export tools
                  </a>
                  <a href={`/compare/${c.slug}`} className="btn btn-primary btn-sm">
                    Full comparison
                  </a>
                </div>
              </div>
            </li>
          ))}
        </ul>
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
