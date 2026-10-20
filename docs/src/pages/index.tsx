// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type {ReactNode} from 'react';
import clsx from 'clsx';
import Link from '@docusaurus/Link';
import useBaseUrl from '@docusaurus/useBaseUrl';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import Heading from '@theme/Heading';

import {useDocLink} from '../docLink';
import styles from './index.module.css';

type Audience = {
  title: string;
  /** Doc path, e.g. `category/devops`. */
  to: string;
  description: string;
  points: string[];
};

const audiences: Audience[] = [
  {
    title: 'Deploy & operate',
    to: 'category/devops',
    description: 'For DevOps and SRE teams.',
    points: [
      'Docker, Compose or AWS Lambda',
      'PostgreSQL, MySQL, SQLite, MongoDB, DynamoDB, RocksDB',
      'S3-compatible media (Garage, MinIO, AWS S3) and Typesense',
    ],
  },
  {
    title: 'Build with the API',
    to: 'category/developers',
    description: 'For front-end and back-end developers.',
    points: [
      'A GraphQL schema generated from your content models',
      'Filters, sorting, search, media and relations',
      'Users, roles, API tokens and a review workflow',
    ],
  },
  {
    title: 'Extend with plugins',
    to: 'category/plugins',
    description: 'For developers adding their own logic.',
    points: [
      'Sandboxed WebAssembly, installed at runtime',
      'Write them in Rust, Go or JavaScript',
      'Validate, transform and react to content changes',
    ],
  },
];

function Hero(): ReactNode {
  const {siteConfig} = useDocusaurusContext();
  const doc = useDocLink();
  return (
    <header className={clsx('hero', styles.heroBanner)}>
      <div className="container">
        <img src={useBaseUrl('/img/logo.svg')} alt="" width={120} height={123} className="margin-bottom--md" />
        <Heading as="h1" className="hero__title">
          {siteConfig.title}
        </Heading>
        <p className="hero__subtitle">{siteConfig.tagline}</p>
        <div className={styles.buttons}>
          <Link className="button button--primary button--lg" to={doc('getting-started/quick-start')}>
            Quick start: 5 minutes
          </Link>
          <Link className="button button--outline button--primary button--lg" to={doc('intro')}>
            What is Tess?
          </Link>
        </div>
      </div>
    </header>
  );
}

export default function Home(): ReactNode {
  const doc = useDocLink();
  return (
    <Layout
      title="Documentation"
      description="Tess is a self-hostable, GraphQL-first headless CMS written in Rust.">
      <Hero />
      <div className="tess-stripe" />
      <main className="container margin-vert--xl">
        <div className="row">
          {audiences.map((a) => (
            <div key={a.title} className="col col--4 margin-bottom--lg">
              <div className="card shadow--md" style={{height: '100%'}}>
                <div className="card__header">
                  <Heading as="h2">{a.title}</Heading>
                  <p>{a.description}</p>
                </div>
                <div className="card__body">
                  <ul>
                    {a.points.map((p) => (
                      <li key={p}>{p}</li>
                    ))}
                  </ul>
                </div>
                <div className="card__footer">
                  <Link className="button button--primary button--block" to={doc(a.to)}>
                    Read the guide
                  </Link>
                </div>
              </div>
            </div>
          ))}
        </div>
        <p className="text--center margin-top--lg">
          Tess comes in a <Link to={doc('editions')}>Community edition</Link> for personal use and an{' '}
          <Link to={doc('editions')}>Enterprise edition</Link> for commercial use.
        </p>
      </main>
    </Layout>
  );
}
