// app.js — self-contained documentation behaviour: a hand-written Flint
// syntax highlighter, sidebar search, active-link tracking, and a mobile
// nav toggle. No external dependencies; works from file://.
(function () {
  "use strict";

  // ------------------------------------------------------------------
  // Syntax highlighting for Flint source.
  // ------------------------------------------------------------------
  var KEYWORDS = {
    "int":1,"string":1,"boolean":1,"byte":1,"short":1,"long":1,
    "char":1,"void":1,"class":1,"interface":1,"abstract":1,"enum":1,
    "package":1,"import":1,"static":1,"public":1,"private":1,
    "if":1,"else":1,"for":1,"while":1,"return":1,"new":1,"this":1,
    "super":1,"break":1,"continue":1,"null":1,"true":1,"false":1,
    "throw":1,"try":1,"catch":1,"finally":1,"instanceof":1,"switch":1,
    "case":1,"default":1,"extends":1,"implements":1,"get":1,"set":1,
    "getset":1,"list":1,"queue":1,"hashmap":1,"hashset":1,"dictionary":1
  };

  // Token kinds are recognised with a single regex pass. Order matters:
  // comments and strings are matched before identifiers so their contents
  // are never re-tokenised.
  var TOKEN_RE = new RegExp([
    "(\\/\\/[^\\n]*|#[^\\n]*)",          // 1 comment
    "(\"(?:\\\\.|[^\"\\\\])*\\\")",       // 2 string (with closing quote)
    "(0[xX][0-9a-fA-F]+)",                // 3 hex number
    "(\\d+(?:\\.\\d+)?)",                // 4 number
    "([A-Za-z_][A-Za-z0-9_]*)",          // 5 identifier / keyword
    "(\\s+)"                              // 6 whitespace
  ].join("|"), "g");

  function escapeHtml(s) {
    return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  }

  function classForWord(word) {
    if (KEYWORDS[word]) return "token-kw";
    // type-like: starts uppercase (class / namespace names)
    if (/[A-Z]/.test(word[0])) return "token-type";
    return null;
  }

  // Highlight a block of Flint source; returns an HTML string.
  function highlightFlint(src) {
    var out = "";
    var last = 0;
    var m;
    TOKEN_RE.lastIndex = 0;
    while ((m = TOKEN_RE.exec(src)) !== null) {
      var i = m.index;
      if (i > last) out += escapeHtml(src.slice(last, i));
      if (m[1] !== undefined) {
        out += '<span class="token-com">' + escapeHtml(m[1]) + "</span>";
      } else if (m[2] !== undefined) {
        out += '<span class="token-str">' + escapeHtml(m[2]) + "</span>";
      } else if (m[3] !== undefined || m[4] !== undefined) {
        out += '<span class="token-num">' + escapeHtml(m[3] || m[4]) + "</span>";
      } else if (m[5] !== undefined) {
        var cls = classForWord(m[5]);
        if (cls) out += '<span class="' + cls + '">' + m[5] + "</span>";
        else out += escapeHtml(m[5]);
      } else if (m[6] !== undefined) {
        out += m[6];
      }
      last = i + m[0].length;
      if (m[0].length === 0) TOKEN_RE.lastIndex++;
    }
    if (last < src.length) out += escapeHtml(src.slice(last));
    return out;
  }

  // ------------------------------------------------------------------
  // Apply highlighting to every pre>code.flint on load.
  // ------------------------------------------------------------------
  function initHighlight() {
    var nodes = document.querySelectorAll("pre code.flint");
    for (var i = 0; i < nodes.length; i++) {
      var el = nodes[i];
      el.innerHTML = highlightFlint(el.textContent);
    }
  }

  // ------------------------------------------------------------------
  // Sidebar search: filter nav items by their text.
  // ------------------------------------------------------------------
  function initSearch() {
    var box = document.getElementById("nav-search");
    if (!box) return;
    box.addEventListener("input", function () {
      var q = box.value.trim().toLowerCase();
      var items = document.querySelectorAll(".nav li");
      for (var i = 0; i < items.length; i++) {
        var a = items[i].querySelector("a");
        var text = a ? a.textContent.toLowerCase() : "";
        items[i].classList.toggle("hidden", q !== "" && text.indexOf(q) === -1);
      }
    });
  }

  // ------------------------------------------------------------------
  // Active link: scrollspy — highlight the section currently in view.
  // ------------------------------------------------------------------
  function initScrollspy() {
    var sections = [];
    var list = document.querySelectorAll("main section[id]");
    for (var i = 0; i < list.length; i++) sections.push(list[i]);
    if (sections.length === 0) return;
    var links = {};
    var navLinks = document.querySelectorAll(".nav a");
    for (var j = 0; j < navLinks.length; j++) {
      links[navLinks[j].getAttribute("href").slice(1)] = navLinks[j];
    }
    var current = null;
    function top(el) {
      return el.getBoundingClientRect().top + window.pageYOffset;
    }
    function onScroll() {
      var y = window.pageYOffset + 140;
      var active = sections[0];
      for (var k = 0; k < sections.length; k++) {
        if (top(sections[k]) <= y) active = sections[k];
        else break;
      }
      var slug = active.id;
      if (slug !== current) {
        current = slug;
        for (var id in links) {
          links[id].classList.toggle("active", id === slug);
        }
      }
    }
    var ticking = false;
    window.addEventListener("scroll", function () {
      if (!ticking) {
        ticking = true;
        requestAnimationFrame(function () { onScroll(); ticking = false; });
      }
    }, { passive: true });
    window.addEventListener("resize", onScroll);
    onScroll();
  }

  // ------------------------------------------------------------------
  // "Copy" button on each code block (best-effort, no dependencies).
  // ------------------------------------------------------------------
  function initCopy() {
    var blocks = document.querySelectorAll(".code");
    for (var i = 0; i < blocks.length; i++) {
      (function (block) {
        var head = block.querySelector(".head");
        var btn = document.createElement("button");
        btn.textContent = "copy";
        btn.style.cssText =
          "background:transparent;border:1px solid var(--border);color:var(--fg-dim);" +
          "border-radius:6px;padding:2px 10px;cursor:pointer;font-size:12px;";
        btn.addEventListener("click", function () {
          var code = block.querySelector("pre");
          var text = code ? code.textContent : "";
          if (navigator.clipboard && navigator.clipboard.writeText) {
            navigator.clipboard.writeText(text).catch(function () {});
          }
          btn.textContent = "copied";
          setTimeout(function () { btn.textContent = "copy"; }, 1200);
        });
        if (head) head.appendChild(btn);
      })(blocks[i]);
    }
  }

  function ready(fn) {
    if (document.readyState !== "loading") fn();
    else document.addEventListener("DOMContentLoaded", fn);
  }

  ready(function () {
    initHighlight();
    initSearch();
    initScrollspy();
    initCopy();
  });
})();
