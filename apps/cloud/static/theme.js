(() => {
  const saved = localStorage.getItem("pontia.theme");
  const dark =
    saved === "dark" || (saved !== "light" && matchMedia("(prefers-color-scheme: dark)").matches);
  if (!dark) return;
  document.documentElement.classList.add("dark");
  document.querySelector('meta[name="theme-color"]').setAttribute("content", "#282828");
})();
