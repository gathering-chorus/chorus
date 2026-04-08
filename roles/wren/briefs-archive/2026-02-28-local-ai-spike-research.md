# Brief: Local AI Spike Research — Options Matrix (#83)

**From:** Silas (Architect)
**To:** Wren (PM)
**Card:** #83 — Local AI for Self domain
**Priority:** P2

## Summary

Research complete. The local AI landscape is mature enough for Jeff's use case. There are strong non-Meta options at every layer. Claude has no local option and won't for the foreseeable future. The recommendation is MLX + Mistral or Phi on the hardware we already own.

---

## 1. Inference Engines (Alternatives to Ollama)

| Engine | What It Is | Trust/Privacy | Apple Silicon | Complexity |
|--------|-----------|---------------|--------------|------------|
| **MLX** | Apple's native ML framework | Apple-built, zero-copy unified memory, no third-party deps | **Best** — 20-30% faster than llama.cpp, native Metal | Medium (Python CLI) |
| **Llamafile** | Mozilla single-executable LLM | Zero network, zero install, fully offline. Mozilla-backed | Good | **Easiest** — double-click and go |
| **llama.cpp** | Pure C/C++ inference, no deps | Open source, fully auditable, no telemetry | Good Metal support | Medium (CLI) |
| **LM Studio** | GUI app (llama.cpp/MLX under hood) | Local-only, no telemetry claimed | Good, Vulkan offloading | Very easy (GUI) |
| **Ollama** | High-level runtime on llama.cpp | Open source, local. The incumbent | Good Metal/CUDA | Very easy (CLI) |
| **LocalAI** | Full AI stack (text/image/audio) | Open source, self-hosted | Supports Apple Silicon | More complex |

**Key insight:** Ollama is just a wrapper around llama.cpp. The real choice is between llama.cpp (cross-platform, battle-tested) and MLX (Apple-native, faster on our hardware). Llamafile is the trust maximizer — single binary, Mozilla stewardship, verifiably no network.

## 2. Non-Meta Models

| Model | Provider | License | Fits M1 16GB? | Fits M2 Pro 32GB? | Trust Profile |
|-------|----------|---------|--------------|-------------------|--------------|
| **Mistral 7B / Mixtral** | Mistral AI (France) | Apache 2.0 | 7B yes | Mixtral 8x7B yes | French company, EU data regs, strong open-source track record |
| **Phi-3 / Phi-4** | Microsoft | MIT | 14B yes (Q4) | 14B comfortably | MIT license, trained on synthetic/curated data, fully open weights |
| **Gemma 2** | Google | Permissive (custom) | 9B yes | 27B yes | Open weights, some usage restrictions in license terms |
| **Qwen 2.5 / 3** | Alibaba (China) | Apache 2.0 | 8B yes | 32B yes | Strong models, but Chinese company — trust bar question for Self domain |
| **DeepSeek** | DeepSeek AI (China) | MIT | Various sizes | Yes | Impressive capability, but same China trust question |

**Trust filter for Self domain:** Given DEC-027 (concentric trust, Self = innermost ring), I'd recommend filtering to EU and US providers only: **Mistral** and **Phi**. Not because Qwen/DeepSeek are bad — they're excellent models — but because the trust bar for Self is the highest, and Jeff should be able to reason about the accountability framework of the model provider.

## 3. Does Claude Offer a Local Option?

**No.** Anthropic has no on-device or local inference product, and nothing on their public roadmap suggests one. Claude is cloud-only.

- **API (commercial terms):** Data is not used for training, but it still leaves your machine. Not viable for Self domain's local-only constraint.
- **Consumer terms (2025 update):** Opted-in data now retained up to 5 years for training. Even more reason to not route Self data through Claude.

Claude is the right tool for Chorus and Gathering (cloud ring in the trust model). It is not the right tool for Self.

## 4. Hardware Viability

**Library Mac mini M1, 16GB:**
- Sweet spot: 7B-8B models at Q4_K_M quantization (~5GB model weight, leaves room for context)
- Best pick: **Qwen3 8B** or **Mistral 7B** — both excellent at instruction following
- With trust filter: **Mistral 7B** or **Phi-4 Mini**
- MLX runs ~20-30% faster than llama.cpp on this hardware

**Bedroom Mac mini M2 Pro, 32GB:**
- Can run 14B-32B models comfortably
- Best pick: **Phi-4 14B** (full precision) or **Mixtral 8x7B** (MoE, fast)
- This machine has the headroom for the "good" model while Library runs the "fast" model
- Memory bandwidth is the bottleneck for LLM inference — M2 Pro's 200GB/s is solid

**Recommendation:** Run the reflection/understanding workload on Bedroom (bigger model, better quality) and keep Library focused on infrastructure. The Self domain's AI doesn't need to be co-located with the app stack.

## 5. Recommendation

| Layer | Pick | Why |
|-------|------|-----|
| **Engine** | MLX (primary), Llamafile (zero-trust fallback) | MLX is fastest on our hardware, Apple-built. Llamafile for maximum verifiability. |
| **Model** | Mistral 7B (Library) + Phi-4 14B (Bedroom) | Non-Meta, permissive licenses, EU/US provenance, proven quality |
| **Machine** | Bedroom (M2 Pro 32GB) for primary inference | More RAM, better bandwidth, separates AI compute from app infra |
| **Interface** | OpenAI-compatible API (all engines expose this) | Any client can connect. Future-proof against engine swaps. |

**What this doesn't solve yet:** The "what does the Self AI actually do?" question. This spike covers the *how* — engine, model, hardware. The *what* (reflection prompts, journal analysis, pattern recognition on life practices) is Wren's product design call.

## Sources

- [Local LLM Hosting Comparison 2026](https://www.glukhov.org/post/2025/11/hosting-llms-ollama-localai-jan-lmstudio-vllm-comparison/)
- [Best Local LLMs for Mac 2026](https://www.insiderllm.com/guides/best-local-llms-mac-2026/)
- [MLX vs llama.cpp Study](https://arxiv.org/abs/2511.05502)
- [Ollama Alternatives Guide](https://localllm.in/blog/complete-guide-ollama-alternatives)
- [Llamafile — Mozilla](https://github.com/mozilla-ai/llamafile)
- [Best Open Source LLMs 2025](https://huggingface.co/blog/daya-shankar/open-source-llms)
- [Apple Silicon LLM Benchmarks](https://apxml.com/posts/best-local-llms-apple-silicon-mac)
