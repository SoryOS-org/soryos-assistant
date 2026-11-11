import { MODEL_FIELD_NAME } from "../constants.js";
import {
  populateModelSelect,
  ensureModelOption,
  currentModelIdFromForm,
  syncModelSelectValue,
} from "../catalog.js";
import { configState } from "../state.js";

function formField(form, name) {
  return form?.elements?.[name] || form?.querySelector(`[name="${name}"]`) || null;
}

export function createConfigEditor({
  form,
  statusElement,
  patchPreview,
}) {
  const setStatus = (message, tone = "info") => {
    if (!statusElement) {
      return;
    }

    statusElement.textContent = message || "";
    statusElement.classList.remove(
      "status-message--error",
      "status-message--success",
      "status-message--info",
    );

    if (!message) {
      return;
    }

    const toneClass =
      tone === "error"
        ? "status-message--error"
        : tone === "success"
          ? "status-message--success"
          : "status-message--info";
    statusElement.classList.add(toneClass);
  };

  const clearStatus = () => setStatus("");

  const spriteScaleOutput = form?.querySelector("[data-sprite-scale-output]");
  const updateSpriteScaleIndicator = (value) => {
    if (!spriteScaleOutput) {
      return;
    }
    const numeric = Number.isFinite(value) ? value : 1;
    spriteScaleOutput.textContent = `${Math.round(numeric * 100)}%`;
  };

  const populateFormFromSummary = (summary) => {
    if (!form || !summary || !summary.stt) {
      return;
    }

    const stt = summary.stt;
    const model = stt.model || {};
    const vad = stt.vad || {};
    const wakeword = stt.wakeword || {};
    const hotkey = stt.hotkey || {};
    const telemetry = stt.telemetry || {};
    const sprite = stt.sprite || {};
    const whisper = stt.whisper || {};
    const modelId = model.id || "";
    const vadAggField = formField(form, "vad.aggressiveness");
    const vadAggOutput = form?.querySelector("[data-vad-agg-output]");
    const wakewordSensitivityField = formField(form, "wakeword.sensitivity");
    const wakewordSensitivityOutput = form?.querySelector("[data-wakeword-sensitivity-output]");

    const providerField = formField(form, "provider");
    if (providerField) {
      const currentProvider = stt.provider || providerField.options?.[0]?.value || "local";
      if (!Array.from(providerField.options).some((option) => option.value === currentProvider)) {
        const fallbackOption = document.createElement("option");
        fallbackOption.value = currentProvider;
        fallbackOption.textContent = currentProvider;
        providerField.appendChild(fallbackOption);
      }
      providerField.value = currentProvider;
    }

    const modelSelect = formField(form, MODEL_FIELD_NAME);
    if (modelSelect) {
      syncModelSelectValue(modelSelect, modelId);
    } else {
      const fallbackSelect = formField(form, MODEL_FIELD_NAME);
      populateModelSelect(fallbackSelect);
      ensureModelOption(fallbackSelect, modelId);
    }

    const pairs = [
      ["vad.aggressiveness", vad.aggressiveness ?? ""],
      ["vad.start_grace_ms", vad.start_grace_ms ?? ""],
      ["vad.max_segment_ms", vad.max_segment_ms ?? ""],
      ["vad.activation_ms", vad.activation_ms ?? ""],
      ["vad.post_speech_silence_ms", vad.post_speech_silence_ms ?? ""],
      ["wakeword.keyword", wakeword.keyword ?? ""],
      ["wakeword.sensitivity", wakeword.sensitivity ?? ""],
      ["hotkey.push_to_talk", hotkey.push_to_talk ?? ""],
    ];

    pairs.forEach(([name, value]) => {
      const field = formField(form, name);
      if (field) {
        field.value = value ?? "";
      }
    });

    if (vadAggField && vadAggOutput) {
      const val = Number.isFinite(vad.aggressiveness) ? vad.aggressiveness : 0;
      vadAggField.value = val;
      vadAggOutput.textContent = val.toString();
    }

    if (wakewordSensitivityField && wakewordSensitivityOutput) {
      const val = Number.isFinite(wakeword.sensitivity) ? wakeword.sensitivity : 0;
      wakewordSensitivityField.value = val;
      wakewordSensitivityOutput.textContent = val.toFixed(2);
    }

    const punctuationField = formField(form, "model.punctuation");
    if (punctuationField) {
      const includePunctuation =
        typeof model.punctuation === "boolean" ? model.punctuation : Boolean(model.punctuation);
      punctuationField.checked = includePunctuation;
    }

    const modeField = formField(form, "hotkey.mode_precedence");
    if (modeField) {
      const currentMode = hotkey.mode_precedence || modeField.options?.[0]?.value || "ptt";
      if (!Array.from(modeField.options).some((option) => option.value === currentMode)) {
        const fallbackOption = document.createElement("option");
        fallbackOption.value = currentMode;
        fallbackOption.textContent = currentMode;
        modeField.appendChild(fallbackOption);
      }
      modeField.value = currentMode;
    }

    const telemetryField = formField(form, "telemetry.level");
    if (telemetryField) {
      const currentLevel = telemetry.level || telemetryField.options?.[0]?.value || "info";
      if (!Array.from(telemetryField.options).some((option) => option.value === currentLevel)) {
        const fallbackOption = document.createElement("option");
        fallbackOption.value = currentLevel;
        fallbackOption.textContent = currentLevel;
        telemetryField.appendChild(fallbackOption);
      }
      telemetryField.value = currentLevel;
    }

    const spriteField = formField(form, "sprite.scale");
    if (spriteField) {
      if (Number.isFinite(sprite.min_scale)) {
        spriteField.min = sprite.min_scale.toString();
      }
      if (Number.isFinite(sprite.max_scale)) {
        spriteField.max = sprite.max_scale.toString();
      }
      const scaleValue = Number.isFinite(sprite.scale) ? sprite.scale : 1;
      spriteField.value = scaleValue.toString();
      updateSpriteScaleIndicator(scaleValue);
    } else {
      updateSpriteScaleIndicator(sprite.scale);
    }

    const whisperThreadsField = formField(form, "whisper.threads");
    if (whisperThreadsField) {
      if (Number.isFinite(whisper.threads)) {
        whisperThreadsField.value = whisper.threads;
      } else {
        whisperThreadsField.value = "";
      }

      const physicalCores = Number.isFinite(whisper.physical_cores)
        ? whisper.physical_cores
        : null;
      const logicalCores = Number.isFinite(whisper.logical_cores)
        ? whisper.logical_cores
        : null;

      if (Number.isFinite(whisper.recommended_threads)) {
        whisperThreadsField.placeholder = whisper.recommended_threads.toString();
      } else {
        whisperThreadsField.placeholder = "Auto";
      }

      if (physicalCores) {
        whisperThreadsField.max = physicalCores.toString();
      } else if (logicalCores) {
        whisperThreadsField.max = logicalCores.toString();
      } else {
        whisperThreadsField.removeAttribute("max");
      }

      if (Number.isFinite(whisper.recommended_threads) && physicalCores) {
        whisperThreadsField.title = `Recommended ${whisper.recommended_threads} threads (physical cores ${physicalCores})`;
      } else if (Number.isFinite(whisper.recommended_threads)) {
        whisperThreadsField.title = `Recommended ${whisper.recommended_threads} threads.`;
      } else if (physicalCores) {
        whisperThreadsField.title = `Up to ${physicalCores} physical cores available; leave blank to auto-tune.`;
      } else {
        whisperThreadsField.title = "Leave blank to auto-tune thread usage.";
      }

      const threadsLabel = document.getElementById("whisper-threads-label");
      if (threadsLabel) {
        const baseLabel =
          threadsLabel.dataset.baseLabel ||
          threadsLabel.textContent.replace(/\s*\(.*\)$/, "").trim() ||
          "Threads";
        threadsLabel.dataset.baseLabel = baseLabel;

        const maxThreads = physicalCores || logicalCores;
        threadsLabel.textContent = maxThreads
          ? `${baseLabel} (max ${maxThreads})`
          : baseLabel;
      }
    }
  };

  const parseIntField = (name) => {
    const field = formField(form, name);
    if (!field) {
      return null;
    }
    const parsed = Number.parseInt(field.value, 10);
    return Number.isFinite(parsed) ? parsed : null;
  };

  const parseFloatField = (name) => {
    const field = formField(form, name);
    if (!field) {
      return null;
    }
    const parsed = Number.parseFloat(field.value);
    return Number.isFinite(parsed) ? parsed : null;
  };

  const parseBooleanField = (name) => {
    const field = formField(form, name);
    if (!field) {
      return null;
    }
    if (field.type === "checkbox") {
      return field.checked;
    }
    const value = field.value;
    if (typeof value === "string") {
      const normalized = value.trim().toLowerCase();
      if (normalized === "true" || normalized === "1") {
        return true;
      }
      if (normalized === "false" || normalized === "0") {
        return false;
      }
    }
    return field.checked ?? null;
  };

  if (form) {
    const spriteScaleField = formField(form, "sprite.scale");
    spriteScaleField?.addEventListener("input", () => {
      const parsed = Number.parseFloat(spriteScaleField.value);
      updateSpriteScaleIndicator(Number.isFinite(parsed) ? parsed : 1);
    });

    const vadAggField = formField(form, "vad.aggressiveness");
    const vadAggOutput = form.querySelector("[data-vad-agg-output]");
    vadAggField?.addEventListener("input", () => {
      const val = Number.parseInt(vadAggField.value, 10);
      if (vadAggOutput) {
        vadAggOutput.textContent = Number.isFinite(val) ? val.toString() : "0";
      }
    });

    const wakewordSensitivityField = formField(form, "wakeword.sensitivity");
    const wakewordSensitivityOutput = form.querySelector("[data-wakeword-sensitivity-output]");
    wakewordSensitivityField?.addEventListener("input", () => {
      const val = Number.parseFloat(wakewordSensitivityField.value);
      if (wakewordSensitivityOutput) {
        wakewordSensitivityOutput.textContent = Number.isFinite(val) ? val.toFixed(2) : "0.00";
      }
    });
  }

  const collectFormValues = () => {
    if (!form) {
      return null;
    }

    return {
      provider: formField(form, "provider")?.value?.trim() ?? "",
      model: {
        id: currentModelIdFromForm(form),
        punctuation: parseBooleanField("model.punctuation"),
      },
      vad: {
        aggressiveness: parseIntField("vad.aggressiveness"),
        start_grace_ms: parseIntField("vad.start_grace_ms"),
        max_segment_ms: parseIntField("vad.max_segment_ms"),
        activation_ms: parseIntField("vad.activation_ms"),
        post_speech_silence_ms: parseIntField("vad.post_speech_silence_ms"),
      },
      wakeword: {
        keyword: formField(form, "wakeword.keyword")?.value?.trim() ?? "",
        sensitivity: parseFloatField("wakeword.sensitivity"),
      },
      hotkey: {
        push_to_talk: formField(form, "hotkey.push_to_talk")?.value?.trim() ?? "",
        mode_precedence: formField(form, "hotkey.mode_precedence")?.value ?? "",
      },
      telemetry: {
        level: formField(form, "telemetry.level")?.value ?? "",
      },
      sprite: {
        scale: parseFloatField("sprite.scale"),
      },
      whisper: {
        threads: parseIntField("whisper.threads"),
      },
    };
  };

  const buildPatchFromValues = (values) => {
    const baseline = configState.summary;
    if (!baseline || !baseline.stt || !values) {
      return {};
    }

    const sttPatch = {};

    const assign = (segments, newValue, oldValue) => {
      const newIsNil = newValue === null || newValue === undefined;
      const oldIsNil = oldValue === null || oldValue === undefined;
      if (newIsNil && oldIsNil) {
        return;
      }
      let valuesEqual;
      if (newIsNil) {
        valuesEqual = oldIsNil;
      } else if (typeof newValue === "number" && typeof oldValue === "number") {
        valuesEqual = Object.is(newValue, oldValue);
      } else {
        valuesEqual = newValue === oldValue;
      }
      if (valuesEqual) {
        return;
      }

      let cursor = sttPatch;
      for (let i = 0; i < segments.length - 1; i += 1) {
        const key = segments[i];
        if (!cursor[key]) {
          cursor[key] = {};
        }
        cursor = cursor[key];
      }

      cursor[segments[segments.length - 1]] = newIsNil ? null : newValue;
    };

    assign(["provider"], values.provider, baseline.stt.provider);

    const baselineModel = baseline.stt.model || {};
    assign(["model", "id"], values.model.id, baselineModel.id);
    assign(["model", "punctuation"], values.model.punctuation, baselineModel.punctuation);

    const baselineVad = baseline.stt.vad || {};
    assign(["vad", "aggressiveness"], values.vad.aggressiveness, baselineVad.aggressiveness);
    assign(["vad", "start_grace_ms"], values.vad.start_grace_ms, baselineVad.start_grace_ms);
    assign(["vad", "max_segment_ms"], values.vad.max_segment_ms, baselineVad.max_segment_ms);
    assign(["vad", "activation_ms"], values.vad.activation_ms, baselineVad.activation_ms);
    assign(
      ["vad", "post_speech_silence_ms"],
      values.vad.post_speech_silence_ms,
      baselineVad.post_speech_silence_ms,
    );

    const baselineWakeword = baseline.stt.wakeword || {};
    assign(["wakeword", "keyword"], values.wakeword.keyword, baselineWakeword.keyword);
    assign(["wakeword", "sensitivity"], values.wakeword.sensitivity, baselineWakeword.sensitivity);

    const baselineHotkey = baseline.stt.hotkey || {};
    assign(["hotkey", "push_to_talk"], values.hotkey.push_to_talk, baselineHotkey.push_to_talk);
    assign(["hotkey", "mode_precedence"], values.hotkey.mode_precedence, baselineHotkey.mode_precedence);

    const baselineTelemetry = baseline.stt.telemetry || {};
    assign(["telemetry", "level"], values.telemetry.level, baselineTelemetry.level);

    const baselineSprite = baseline.stt.sprite || {};
    assign(["sprite", "scale"], values.sprite.scale, baselineSprite.scale);

    const baselineWhisper = baseline.stt.whisper || {};
    assign(["whisper", "threads"], values.whisper.threads, baselineWhisper.threads);

    if (Object.keys(sttPatch).length === 0) {
      return {};
    }

    return sttPatch;
  };

  const hasChanges = (patch) => {
    if (patch === null || patch === undefined) {
      return false;
    }

    if (typeof patch === "object" && !Array.isArray(patch)) {
      return Object.values(patch).some((value) => hasChanges(value));
    }

    return true;
  };

  const updatePatchPreview = () => {
    if (!form) {
      return;
    }

    const values = collectFormValues();
    const patch = buildPatchFromValues(values);
    configState.pendingPatch = patch;

    if (patchPreview) {
      if (hasChanges(patch)) {
        patchPreview.textContent = JSON.stringify(patch, null, 2);
      } else {
        patchPreview.textContent = "// No changes";
      }
    }

    if (hasChanges(patch)) {
      setStatus("Review the preview and click Save to apply.", "info");
    } else {
      clearStatus();
    }
  };

  return {
    setStatus,
    clearStatus,
    populateFormFromSummary,
    collectFormValues,
    buildPatchFromValues,
    updatePatchPreview,
    hasChanges,
  };
}
