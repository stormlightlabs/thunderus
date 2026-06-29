(function () {
  try {
    var html = document.documentElement;
    html.classList.add("coal");
    localStorage.setItem("mdbook-theme", "coal");
  } catch (_) {
  }
})();
