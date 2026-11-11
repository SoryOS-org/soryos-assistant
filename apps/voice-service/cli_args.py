import argparse
import logging
import os
import sys
from dataclasses import dataclass
from functools import lru_cache
from typing import Optional


@dataclass(frozen=True)
class CliArgs:
    log_dir: str
    wakeword: Optional[str]
    sensitivity: Optional[float]
    access_key: Optional[str]
    parent_pid: Optional[int]
    app_user_model_id: Optional[str]


def _ensure_log_dir(path: Optional[str]) -> str:
    candidate = path or os.getenv("VOICE_PROCESSOR_LOG_DIR")
    if candidate:
        try:
            os.makedirs(candidate, exist_ok=True)
            return candidate
        except OSError:
            pass
    fallback = os.getcwd()
    os.makedirs(fallback, exist_ok=True)
    return fallback


def _parse_parent_pid(value: Optional[str]) -> Optional[int]:
    if value is None:
        return None
    try:
        candidate = int(str(value).strip())
        if candidate <= 0:
            raise ValueError
        return candidate
    except (TypeError, ValueError):
        return None


@lru_cache(maxsize=1)
def get_cli_args() -> CliArgs:
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--log-dir", dest="log_dir", default=None)
    parser.add_argument("--wakeword", dest="wakeword", default=None)
    parser.add_argument("--sensitivity", dest="sensitivity", type=float, default=None)
    parser.add_argument("--pv-access-key", dest="access_key", default=None)
    parser.add_argument("--parent-pid", dest="parent_pid_flag", default=None)
    parser.add_argument("--app-user-model-id", dest="app_user_model_id", default=None)
    parser.add_argument("parent_pid", nargs="?", default=None)

    known_args, _ = parser.parse_known_args(sys.argv[1:])

    parent_pid: Optional[int] = _parse_parent_pid(known_args.parent_pid_flag)

    if parent_pid is None and known_args.parent_pid is not None:
        parent_pid = _parse_parent_pid(known_args.parent_pid)
        if parent_pid is None:
            logging.getLogger(__name__).warning(
                "Invalid parent PID argument: %s", known_args.parent_pid
            )

    return CliArgs(
        log_dir=_ensure_log_dir(known_args.log_dir),
        wakeword=known_args.wakeword,
        sensitivity=known_args.sensitivity,
        access_key=known_args.access_key,
        parent_pid=parent_pid,
        app_user_model_id=_normalize_app_user_model_id(known_args.app_user_model_id),
    )


def _normalize_app_user_model_id(value: Optional[str]) -> Optional[str]:
    if value is None:
        return None
    trimmed = value.strip()
    return trimmed or None
