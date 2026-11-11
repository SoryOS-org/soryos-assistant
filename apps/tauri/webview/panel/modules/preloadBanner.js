export function createPreloadBanner({
  banner,
  message,
  progressContainer,
  progressBar,
}) {
  const setProgress = (percent) => {
    if (!progressContainer || !progressBar) {
      return;
    }
    const clamped = Math.max(0, Math.min(percent, 100));
    progressBar.style.width = `${clamped}%`;
    progressContainer.setAttribute("aria-valuenow", clamped.toFixed(0));
    progressContainer.setAttribute("aria-valuetext", `${Math.round(clamped)}%`);
  };

  const setMessage = (text) => {
    if (message) {
      message.textContent = text;
    }
  };

  const show = () => {
    if (banner) {
      banner.hidden = false;
    }
  };

  const hide = () => {
    if (banner) {
      banner.hidden = true;
      banner.classList.remove("panel-preload--error");
    }
  };

  const showError = (text) => {
    if (banner) {
      banner.hidden = false;
      banner.classList.add("panel-preload--error");
    }
    setMessage(text);
    setProgress(0);
  };

  return {
    handle(payload = {}) {
      if (!banner) {
        return;
      }
      const { state, model, progress, details } = payload;
      const label = model ? model : "requested model";

      switch (state) {
        case "started":
          banner.classList.remove("panel-preload--error");
          setMessage(`Downloading ${label}...`);
          setProgress(0);
          show();
          break;
        case "progress": {
          banner.classList.remove("panel-preload--error");
          const percent = typeof progress === "number" ? progress * 100 : 0;
          setMessage(`Downloading ${label}... ${Math.round(percent)}%`);
          setProgress(percent);
          show();
          break;
        }
        case "cached":
        case "completed":
          hide();
          setProgress(100);
          break;
        case "error": {
          const extra = details ? `: ${details}` : ". Check logs for details.";
          showError(`Failed to prepare ${label}${extra}`);
          break;
        }
        default:
          break;
      }
    },
  };
}
