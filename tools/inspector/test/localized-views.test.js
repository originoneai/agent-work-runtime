"use strict";
const { test } = require("node:test");
const assert = require("node:assert/strict");
const { install } = require("./fixtures/dom-stub.js");
const i18n = require("../public/i18n.js");

for (const locale of ["en", "zh-CN"]) {
  test(`pagination and compilation render in ${locale} without translating source content`, async () => {
    install();
    i18n.setLocale(locale);
    delete require.cache[require.resolve("../public/app.js")];
    const app = require("../public/app.js");
    const $ = (id) => document.getElementById(id);
    const original = "Project text α 🧭 $& <script>";
    const brief = { key: "W1", title: original, queue: "ready" };
    app.state.mode = "live";
    app.state.workPagination = true;
    app.state.workFilter = "ready";
    app.state.status = app.normStatus({ ready_count: 11, ready: [brief], current: [], waiting: [], blocked: [] });
    global.fetch = async (url) => ({ json: async () => url.startsWith("/api/work-page")
      ? { ok: true, data: { page: { queue: "ready", offset: 0, limit: 10, total: 11, has_more: true, items: [brief] } } }
      : { ok: true, data: { work: { external_key: "W1", title: original }, acceptance: [original] } } });
    await app.loadWorkPage();
    const pager = $("workPagination").textContent;
    assert.ok(pager.includes(i18n.t("ui.previous_page")));
    assert.ok(pager.includes(i18n.t("ui.next_page")));
    assert.ok(pager.includes(i18n.t("ui.page_summary", { page: 1, pages: 2, total: 11 })));
    assert.ok($("workRows").textContent.includes(original));
    await app.renderWorkDetail("W1");
    assert.ok($("workDetail").textContent.includes(original));

    const option = document.createElement("option");
    option.value = "W1";
    $("fWork").appendChild(option);
    $("fWork").value = "W1";
    $("fBudget").value = "4000";
    global.fetch = async () => ({ json: async () => ({ ok: true, data: {
      completeness: { complete: true },
      work_context: { rendered_context: original, token_estimate: 2000, required_tokens: 1000,
        token_budget: 4000, selected_chunks: [], omitted_chunks: [{ key: "chunk-1", section: "delta", reason: "budget" }] },
    } }) });
    await app.doCompile();
    assert.ok($("sizeChart").textContent.includes(i18n.t("ui.required_content")));
    assert.ok($("sizeChart").textContent.includes(i18n.t("ui.included_content")));
    assert.equal($("sizeSub").textContent, i18n.t("ui.budget_used", { percent: 50 }));
    assert.equal($("omittedSummary").textContent, i18n.t("ui.omitted_ids", { count: 1 }));
    assert.equal($("packetPreview").textContent, original);
    i18n.setLocale("en");
  });
}
