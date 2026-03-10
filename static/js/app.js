/**
 * rusty-board — app.js
 *
 * Progressive enhancement only. The board must be fully functional without JS.
 * This file adds quality-of-life enhancements on top of the baseline HTML experience.
 */

(function () {
  "use strict";

  // ── Quote links ─────────────────────────────────────────────────────────────
  // Clicking >>postId highlights the referenced post (if it's on the page).

  function highlightQuoteTarget(postId) {
    const el = document.getElementById("post-" + postId);
    if (!el) return;

    // Remove existing highlights
    document.querySelectorAll(".post-highlight").forEach(function (n) {
      n.classList.remove("post-highlight");
    });

    el.classList.add("post-highlight");
    el.scrollIntoView({ behavior: "smooth", block: "nearest" });

    // Remove highlight after 2 seconds
    setTimeout(function () {
      el.classList.remove("post-highlight");
    }, 2000);
  }

  document.addEventListener("click", function (e) {
    const link = e.target.closest(".quote-link");
    if (!link) return;
    const href = link.getAttribute("href");
    if (!href || !href.startsWith("#post-")) return;
    const postId = href.slice(6);
    if (document.getElementById("post-" + postId)) {
      e.preventDefault();
      highlightQuoteTarget(postId);
    }
  });

  // ── Image expand ─────────────────────────────────────────────────────────────
  // Clicking a thumbnail expands it to full size in-place.

  document.addEventListener("click", function (e) {
    const thumb = e.target.closest(".attachment img");
    if (!thumb) return;
    e.preventDefault();

    const link = thumb.closest("a.attachment");
    if (!link) return;

    if (thumb.dataset.expanded === "1") {
      // Collapse back to thumbnail
      thumb.style.maxWidth = "";
      thumb.style.maxHeight = "";
      thumb.dataset.expanded = "";
    } else {
      // Expand to full
      thumb.style.maxWidth = "100%";
      thumb.style.maxHeight = "none";
      thumb.dataset.expanded = "1";
    }
  });

  // ── Reply form prefill ────────────────────────────────────────────────────────
  // Clicking >>N in a post pre-fills the reply textarea with a quote reference.

  document.addEventListener("click", function (e) {
    const postNum = e.target.closest("[data-post-number]");
    if (!postNum) return;

    const num = postNum.dataset.postNumber;
    if (!num) return;

    const form = document.querySelector("form[data-reply-form]");
    if (!form) return;

    const textarea = form.querySelector("textarea");
    if (!textarea) return;

    e.preventDefault();
    textarea.value = ">>" + num + "\n" + textarea.value;
    textarea.focus();
    textarea.setSelectionRange(num.length + 3, num.length + 3);
  });

  // ── Auto-resize textarea ──────────────────────────────────────────────────────

  document.querySelectorAll("textarea").forEach(function (ta) {
    ta.addEventListener("input", function () {
      ta.style.height = "auto";
      ta.style.height = Math.min(ta.scrollHeight, 400) + "px";
    });
  });

  // ── Spoiler text toggle ───────────────────────────────────────────────────────

  document.addEventListener("click", function (e) {
    const spoiler = e.target.closest(".spoiler");
    if (spoiler) {
      spoiler.classList.toggle("spoiler-revealed");
    }
  });

})();
