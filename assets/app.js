// HyOrchestrator page helpers (the UI itself is rendered by the Rust server over the LiveView socket):
// drag & drop / file-picker uploads of mods, and a "connection lost" hint.
(function () {
  "use strict";

  function zoneFor(el) {
    return el && el.closest ? el.closest("[data-dropzone]") : null;
  }

  function status(instance, text, cls) {
    var s = document.getElementById("drop-status-" + instance);
    if (s) {
      s.textContent = text;
      s.className = "drop-status " + (cls || "");
    }
  }

  function upload(instance, files) {
    var list = Array.prototype.slice.call(files || []);
    if (!list.length) return;
    var form = new FormData();
    list.forEach(function (f) { form.append("file", f, f.name); });
    status(instance, "Uploading " + list.length + " file(s)…", "busy");
    fetch("/api/instances/" + encodeURIComponent(instance) + "/mods", { method: "POST", body: form, credentials: "same-origin" })
      .then(function (r) { return r.json().catch(function () { return { errors: ["HTTP " + r.status] }; }); })
      .then(function (res) {
        var ok = (res.saved || []).length, errs = res.errors || [];
        if (errs.length) status(instance, (ok ? ok + " added · " : "") + errs.join(" · "), "err");
        else status(instance, ok + " mod(s) added", "ok");
      })
      .catch(function (e) { status(instance, "Upload failed: " + e, "err"); });
  }

  var depth = 0;
  function hasFiles(e) {
    var t = e.dataTransfer && e.dataTransfer.types;
    return t && Array.prototype.indexOf.call(t, "Files") >= 0;
  }

  document.addEventListener("dragenter", function (e) {
    if (!hasFiles(e)) return;
    depth++;
    if (document.querySelector("[data-dropzone]")) document.body.classList.add("dragging");
  });
  document.addEventListener("dragleave", function () {
    depth = Math.max(0, depth - 1);
    if (!depth) document.body.classList.remove("dragging");
  });
  document.addEventListener("dragover", function (e) {
    if (!hasFiles(e)) return;
    e.preventDefault(); // allow dropping (and never let the browser open the file)
    var z = zoneFor(e.target);
    document.querySelectorAll("[data-dropzone].over").forEach(function (x) { if (x !== z) x.classList.remove("over"); });
    if (z) z.classList.add("over");
  });
  document.addEventListener("drop", function (e) {
    if (!hasFiles(e)) return;
    e.preventDefault();
    depth = 0;
    document.body.classList.remove("dragging");
    document.querySelectorAll("[data-dropzone].over").forEach(function (x) { x.classList.remove("over"); });
    // dropped anywhere on a page that has a mod zone: upload to that instance
    var z = zoneFor(e.target) || document.querySelector("[data-dropzone]");
    if (z) upload(z.getAttribute("data-instance"), e.dataTransfer.files);
  });
  document.addEventListener("change", function (e) {
    var t = e.target;
    if (t && t.matches && t.matches("input[type=file][data-upload]")) {
      upload(t.getAttribute("data-instance"), t.files);
      t.value = "";
    }
  }, true);

  // copy buttons (data-copy="text"): sign-in codes. navigator.clipboard needs a secure context
  // (127.0.0.1 is one; a LAN address over http is not), so fall back to a hidden textarea.
  document.addEventListener("click", function (e) {
    var b = e.target && e.target.closest ? e.target.closest("[data-copy]") : null;
    if (!b) return;
    var text = b.getAttribute("data-copy");
    function done() {
      var old = b.textContent;
      b.textContent = "Copied ✓";
      b.classList.add("copied");
      setTimeout(function () { b.textContent = old; b.classList.remove("copied"); }, 1500);
    }
    function fallback() {
      var t = document.createElement("textarea");
      t.value = text; t.style.position = "fixed"; t.style.opacity = "0";
      document.body.appendChild(t); t.select();
      try { document.execCommand("copy"); done(); } catch (_) {}
      document.body.removeChild(t);
    }
    if (navigator.clipboard && window.isSecureContext) navigator.clipboard.writeText(text).then(done, fallback);
    else fallback();
  });

  // connection hint: the LiveView client reconnects by itself
  var Native = window.WebSocket;
  if (Native) {
    window.WebSocket = function (url, proto) {
      var ws = proto ? new Native(url, proto) : new Native(url);
      ws.addEventListener("open", function () { document.body.classList.remove("offline"); });
      ws.addEventListener("close", function () { document.body.classList.add("offline"); });
      return ws;
    };
    window.WebSocket.prototype = Native.prototype;
    ["CONNECTING", "OPEN", "CLOSING", "CLOSED"].forEach(function (k) { window.WebSocket[k] = Native[k]; });
  }
})();
