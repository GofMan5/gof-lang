(function () {
    function ensureProgressBar() {
        if (document.querySelector(".docs-progress")) {
            return;
        }
        const progress = document.createElement("div");
        progress.className = "docs-progress";
        progress.innerHTML = '<div class="docs-progress__bar"></div>';
        document.body.appendChild(progress);
    }

    function updateProgress() {
        const bar = document.querySelector(".docs-progress__bar");
        const main = document.querySelector("main");
        if (!bar || !main) {
            return;
        }

        const scrollTop = window.scrollY || document.documentElement.scrollTop;
        const maxScroll = Math.max(
            main.offsetHeight - window.innerHeight + 160,
            1,
        );
        const progress = Math.min(Math.max(scrollTop / maxScroll, 0), 1);
        bar.style.width = `${progress * 100}%`;
    }

    function tagHomePage() {
        const firstHeading = document.querySelector("main h1");
        if (!firstHeading) {
            return;
        }
        const headingText = firstHeading.textContent.trim().toLowerCase();
        if (headingText === "introduction") {
            document.body.classList.add("docs-home");
        }
    }

    function annotateExternalLinks() {
        document.querySelectorAll("main a[href^='http']").forEach((link) => {
            if (link.dataset.docsDecorated === "true") {
                return;
            }
            link.dataset.docsDecorated = "true";
            link.target = "_blank";
            link.rel = "noreferrer noopener";
        });
    }

    function injectLanguageSwitcher() {
        const menuBarRight = document.querySelector("#mdbook-menu-bar .right-buttons");
        if (!menuBarRight || document.querySelector(".docs-lang-switcher")) {
            return;
        }

        const htmlLang = document.documentElement.lang === "ru" ? "ru" : "en";
        const pathname = window.location.pathname;
        const currentFile = pathname.split("/").filter(Boolean).pop() || "index.html";
        const enHref = htmlLang === "ru" ? `../${currentFile}` : currentFile;
        const ruHref = htmlLang === "ru" ? currentFile : `ru/${currentFile}`;

        const switcher = document.createElement("div");
        switcher.className = "docs-lang-switcher";
        switcher.innerHTML = `
            <a class="docs-lang-switcher__link ${htmlLang === "en" ? "is-active" : ""}" href="${enHref}" lang="en">EN</a>
            <a class="docs-lang-switcher__link ${htmlLang === "ru" ? "is-active" : ""}" href="${ruHref}" lang="ru">RU</a>
        `;
        menuBarRight.prepend(switcher);
    }

    function init() {
        ensureProgressBar();
        tagHomePage();
        annotateExternalLinks();
        injectLanguageSwitcher();
        updateProgress();
        window.addEventListener("scroll", updateProgress, { passive: true });
        window.addEventListener("resize", updateProgress);
    }

    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", init);
    } else {
        init();
    }
})();
