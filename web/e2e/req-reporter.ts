// Reporter dla req-check: wypisuje wyniki w formacie `test <nazwa> ... ok|FAILED`
// (jak cargo test), żeby testy E2E wiązały się z wymaganiami (--test-log).

import type { Reporter, TestCase, TestResult } from "@playwright/test/reporter";

export default class ReqReporter implements Reporter {
  onTestEnd(test: TestCase, result: TestResult) {
    const status = result.status === "passed" ? "ok" : result.status === "skipped" ? "ignored" : "FAILED";
    process.stdout.write(`test ${test.title} ... ${status}\n`);
  }
}

declare const process: { stdout: { write(s: string): void } };
