/* THE CHERRY TREE — docs site behaviour. No dependencies. */
(function () {
  "use strict";

  /* ----- mobile nav ----- */
  var btn = document.querySelector(".menu-btn");
  var sidebar = document.querySelector(".sidebar");
  if (btn && sidebar) {
    btn.addEventListener("click", function () {
      var open = document.body.classList.toggle("nav-open");
      btn.setAttribute("aria-expanded", open ? "true" : "false");
    });
    sidebar.addEventListener("click", function (e) {
      if (e.target.closest("a")) {
        document.body.classList.remove("nav-open");
        btn.setAttribute("aria-expanded", "false");
      }
    });
    document.addEventListener("click", function (e) {
      if (document.body.classList.contains("nav-open")
          && !e.target.closest(".sidebar")
          && !e.target.closest(".menu-btn")) {
        document.body.classList.remove("nav-open");
        btn.setAttribute("aria-expanded", "false");
      }
    });
  }

  /* ----- active nav link ----- */
  var here = (location.pathname.split("/").pop() || "index.html");
  document.querySelectorAll(".sidebar a.page").forEach(function (a) {
    if ((a.getAttribute("href") || "").split("/").pop() === here) {
      a.classList.add("active");
      a.setAttribute("aria-current", "page");
    }
  });

  /* ----- sidebar search filter ----- */
  var q = document.querySelector(".nav-search");
  if (q) {
    q.addEventListener("input", function () {
      var t = q.value.trim().toLowerCase();
      var gh = document.querySelector(".gh-side");
      if (gh) gh.hidden = t !== "";
      document.querySelectorAll(".sidebar .nav-group").forEach(function (g) {
        var n = 0;
        g.querySelectorAll("a.page").forEach(function (a) {
          var hit = a.textContent.toLowerCase().indexOf(t) > -1;
          a.hidden = !hit;
          if (hit) n++;
        });
        g.hidden = n === 0;
      });
    });
  }

  /* ----- right rail TOC + heading anchors ----- */
  var toc = document.querySelector(".toc");
  var hs = document.querySelectorAll("main h2");
  hs.forEach(function (h, i) {
    if (!h.id) {
      h.id = h.textContent.toLowerCase().replace(/[^a-z0-9]+/g, "-")
        .replace(/^-+|-+$/g, "") || "s" + i;
    }
    if (toc) {
      var a = document.createElement("a");
      a.href = "#" + h.id;
      a.textContent = h.textContent;
      toc.appendChild(a);
    }
    var s = document.createElement("a");
    s.className = "anchor";
    s.href = "#" + h.id;
    s.textContent = "#";
    s.setAttribute("aria-label", "Link to this section");
    h.appendChild(s);
  });
  if (toc && !toc.children.length) {
    var rail = toc.closest(".rail");
    if (rail) rail.hidden = true;
  }

  /* ----- copy button on every code block ----- */
  document.querySelectorAll("pre").forEach(function (pre) {
    var b = document.createElement("button");
    b.className = "copy";
    b.type = "button";
    b.textContent = "COPY";
    b.addEventListener("click", function () {
      var done = function () {
        b.textContent = "COPIED";
        setTimeout(function () { b.textContent = "COPY"; }, 1600);
      };
      var txt = pre.querySelector("code")
        ? pre.querySelector("code").innerText
        : pre.innerText;
      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(txt).then(done, done);
      } else {
        var r = document.createRange();
        r.selectNodeContents(pre.querySelector("code") || pre);
        var sel = window.getSelection();
        sel.removeAllRanges();
        sel.addRange(r);
        try { document.execCommand("copy"); } catch (e) {}
        sel.removeAllRanges();
        done();
      }
    });
    pre.appendChild(b);
  });

  /* ----- hyper stats: count the real constants up on load ----- */
  var counters = document.querySelectorAll("[data-count]");
  counters.forEach(function (el) {
    var target = parseFloat(el.getAttribute("data-count"));
    var suf = el.getAttribute("data-suffix") || "";
    var fmt = function (v) {
      return Math.round(v).toLocaleString("en-US") + suf;
    };
    var still = window.matchMedia
      && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (still) { el.textContent = fmt(target); return; }
    var t0 = performance.now(), dur = 900;
    var step = function (now) {
      var p = Math.min(1, (now - t0) / dur);
      var e = 1 - Math.pow(1 - p, 3);
      el.textContent = fmt(target * e);
      if (p < 1) requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  });

  /* ----- hero petals (pure CSS animation, elements made here) ----- */
  var bed = document.querySelector(".petals");
  var calm = window.matchMedia
    && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (bed && !calm) {
    for (var i = 0; i < 13; i++) {
      var p = document.createElement("i");
      var size = 7 + Math.random() * 8;
      p.style.left = (3 + Math.random() * 93) + "%";
      p.style.width = size + "px";
      p.style.height = size * 1.15 + "px";
      p.style.opacity = (0.25 + Math.random() * 0.35).toFixed(2);
      p.style.animationDelay = (-Math.random() * 20).toFixed(2) + "s";
      p.style.animationDuration = (11 + Math.random() * 10).toFixed(2) + "s";
      bed.appendChild(p);
    }
  }
})();
