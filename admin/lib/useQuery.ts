// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useCallback, useEffect, useState } from "react";
import { describeError } from "./graphql";

interface QueryState<T> {
  data: T | undefined;
  error: string | undefined;
  loading: boolean;
}

/** Minimal data loader: runs `load` on mount and whenever `reload()` is called. */
export function useQuery<T>(load: () => Promise<T>) {
  const [state, setState] = useState<QueryState<T>>({ data: undefined, error: undefined, loading: true });
  const [version, setVersion] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setState((s) => ({ ...s, loading: true, error: undefined }));
    load().then(
      (data) => {
        if (!cancelled) setState({ data, error: undefined, loading: false });
      },
      (error: unknown) => {
        if (!cancelled) setState((s) => ({ ...s, error: describeError(error), loading: false }));
      },
    );
    return () => {
      cancelled = true;
    };
    // `load` is expected to be a stable module-level function.
    // oxlint-disable-next-line react-hooks/exhaustive-deps
  }, [version]);

  const reload = useCallback(() => setVersion((v) => v + 1), []);
  const setData = useCallback((update: (data: T) => T) => {
    setState((s) => (s.data === undefined ? s : { ...s, data: update(s.data) }));
  }, []);

  return { ...state, reload, setData };
}
