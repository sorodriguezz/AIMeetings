//! The language models AIMeetings can download and run on the bundled llama.cpp engine, to write
//! minutes and answer questions about a meeting without anything leaving the computer.
//!
//! Every entry is an **instruct** model in **one GGUF file**, under a licence the app can point at
//! (SPDX, or the NVIDIA open model licence with its link). `size_bytes` and `sha256` come from the
//! Hugging Face API (`lfs.size` / `lfs.oid`), and the architecture numbers behind
//! [`ExecModelSpec::kv_bytes_per_token`] from each model's own `config.json`.

/// Which machine an entry is meant for. Purely advisory — nothing refuses to download a model the
/// machine cannot comfortably run — but it is what the settings pane sorts and labels by, and the
/// honest answer to "which one do I pick" for someone who does not follow model releases.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    /// Runs on anything, including a laptop with no usable GPU. Completes a line, not a function.
    Light,
    /// The default. The smallest model that reliably finishes a *block* rather than a line.
    Balanced,
    /// Wants real memory and a real GPU. Noticeably better at code it has to infer rather than
    /// copy from the surrounding lines.
    Large,
}

/// One downloadable model.
///
/// `&'static str` throughout rather than `String`: this is a compile-time constant that is read
/// far more often than it is built, and keeping it borrowed means the catalogue costs no
/// allocation to consult and cannot drift from the binary that ships it.
#[derive(Clone, Copy, Debug)]
pub struct ModelSpec {
    /// Stable across releases. This is what lands in `app_settings` as the active model and what
    /// names the file on disk, so renaming one orphans a user's download.
    pub id: &'static str,
    /// Shown as-is, untranslated. It is the model's name, not a sentence of ours.
    pub label: &'static str,
    pub tier: Tier,
    /// The Hugging Face repository. Combined with [`Self::file`] by [`Self::url`].
    pub repo: &'static str,
    /// The file inside that repository, and the name it keeps under `paths::models_dir()`.
    pub file: &'static str,
    /// Exact, from the HF API. A download that ends at a different length is a truncated download,
    /// which is worth catching before the hash does because it costs nothing to check.
    pub size_bytes: u64,
    /// Lowercase hex SHA-256 of the file's contents.
    pub sha256: &'static str,
    /// For the settings row. Not parsed anywhere.
    pub params: &'static str,
    /// Rough working set for the model plus its KV cache at the context size
    /// the default window launches with. Advisory; see [`Tier`].
    pub min_ram_gb: u32,
    /// SPDX identifier where there is one. Every entry must be a licence this app can offer
    /// without qualification — see rule 3 in the module comment.
    pub licence: &'static str,
}

impl ModelSpec {
    /// Where the weights come from.
    ///
    /// Built rather than stored because the two halves are already fields and a third copy of the
    /// same string is a third place for a typo. Note that this 302s to a CDN host, so whatever
    /// fetches it must follow redirects — see `crate::download`.
    pub fn url(&self) -> String {
        format!("https://huggingface.co/{}/resolve/main/{}", self.repo, self.file)
    }
}

/// One downloadable executor model.
#[derive(Clone, Copy, Debug)]
pub struct ExecModelSpec {
    /// The fields the download pipeline and the on-disk bookkeeping already understand — see
    /// `crate::download::fetch` and [`super::models`], which take a [`ModelSpec`] and neither
    /// knows nor cares which catalogue it came from.
    pub spec: ModelSpec,
    /// The context the model was trained for. The ceiling the settings pane offers: asking
    /// llama-server for more than this runs, and degrades quietly past the trained length.
    pub max_ctx: u32,
    /// KV-cache bytes per token of context at f16: `2 · layers · kv_heads · head_dim · 2`.
    ///
    /// What turns "16k of context" into gigabytes before anything is loaded, which is the only
    /// honest way to tell someone on a 16 GB laptop and someone with a 24 GB GPU what fits.
    pub kv_bytes_per_token: u64,
    /// Total parameters in billions. Read by the delegation suggestion, which hands the local model
    /// harder work the bigger it is.
    pub params_b: f32,
    /// Parameters read for each token, in billions: all of them for a dense model, the routed
    /// experts and the shared layers for a mixture. What the writing speed is estimated from — a
    /// token costs the bytes it reads, not the bytes that are resident.
    pub active_params_b: f32,
    /// Which model family the weights belong to: what a fine-tuning pipeline and the chat template
    /// are matched by. Two families never share an adapter.
    pub family: Family,
    /// The chat template takes tools — *declared*; whether the model calls them well is what the
    /// bench's conformance suite verifies (see `the model registry`).
    pub tools: bool,
    /// The model has a reasoning phase that can be switched on and off.
    pub thinking: bool,
    /// The weights use a quantisation llama.cpp only implements on CUDA. The bundled engine is a
    /// Metal build on macOS and a Vulkan one on Windows, so such a model runs only through Ollama on
    /// an NVIDIA card — the entry stays for its architecture and the row says what it needs.
    pub cuda_only: bool,
    /// Where the licence text lives when [`ModelSpec::licence`] is not an SPDX identifier.
    pub licence_url: &'static str,
    /// Layers that hold experts (0 for a dense model): what `--n-cpu-moe` counts, read off the GGUF
    /// header — see `crate::hardware::budget::cpu_moe_layers`.
    pub moe_layers: u32,
}

/// Which family a model belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    Qwen,
    Phi,
    Nemotron,
}

/// Everything on offer, smallest first — the first row a hesitant user reads is the one that
/// certainly runs on their machine.
pub const EXEC_CATALOGUE: &[ExecModelSpec] = &[
    ExecModelSpec {
        spec: ModelSpec {
            id: "phi-4-mini-instruct",
            label: "Phi-4-mini Instruct",
            tier: Tier::Light,
            repo: "unsloth/Phi-4-mini-instruct-GGUF",
            file: "Phi-4-mini-instruct-Q4_K_M.gguf",
            size_bytes: 2_491_874_272,
            sha256: "88c00229914083cd112853aab84ed51b87bdf6b9ce42f532d8c85c7c63b1730a",
            params: "3.8B · Q4_K_M",
            min_ram_gb: 6,
            licence: "MIT",
        },
        // 4k trained, stretched to 128k with LongRoPE (`rope_scaling` in its config.json).
        max_ctx: 131_072,
        // 32 layers · 8 KV heads · head_dim 128 (3072 / 24). More per token than the 7B's 56 KiB:
        // a small model is not a small context.
        kv_bytes_per_token: 2 * 32 * 8 * 128 * 2,
        params_b: 3.8,
        active_params_b: 3.8,
        family: Family::Phi,
        tools: true,
        thinking: false,
        cuda_only: false,
        licence_url: "",
        moe_layers: 0,
    },
    ExecModelSpec {
        spec: ModelSpec {
            id: "qwen3-4b-instruct-2507",
            label: "Qwen3 4B Instruct",
            tier: Tier::Light,
            // Qwen publishes no GGUF of the 2507 instruct; unsloth's, like the 30B-A3B's below.
            repo: "unsloth/Qwen3-4B-Instruct-2507-GGUF",
            file: "Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
            size_bytes: 2_497_281_120,
            sha256: "3605803b982cb64aead44f6c1b2ae36e3acdb41d8e46c8a94c6533bc4c67e597",
            params: "4.0B · Q4_K_M",
            min_ram_gb: 6,
            licence: "Apache-2.0",
        },
        max_ctx: 262_144,
        // 36 layers · 8 KV heads · head_dim 128 (its config.json states it).
        kv_bytes_per_token: 2 * 36 * 8 * 128 * 2,
        params_b: 4.0,
        active_params_b: 4.0,
        family: Family::Qwen,
        tools: true,
        thinking: false,
        cuda_only: false,
        licence_url: "",
        moe_layers: 0,
    },
    ExecModelSpec {
        spec: ModelSpec {
            id: "nemotron-3-nano-4b",
            label: "Nemotron 3 Nano 4B",
            tier: Tier::Light,
            repo: "nvidia/NVIDIA-Nemotron-3-Nano-4B-GGUF",
            file: "NVIDIA-Nemotron3-Nano-4B-Q4_K_M.gguf",
            size_bytes: 2_837_072_864,
            sha256: "be5d9a656a51922f24f1f09a759cebb694e1f5d9728bf0ef9f8c972c5a0b5ef2",
            params: "4.0B · Q4_K_M",
            min_ram_gb: 6,
            licence: "NVIDIA-Nemotron-Open-Model-License",
        },
        max_ctx: 1_048_576,
        // Hybrid Mamba-2/Transformer: of 42 layers only 4 hold a KV cache (8 heads each, head_dim 128),
        // read off the GGUF header — 16 KiB a token, a tenth of what all-attention would cost.
        kv_bytes_per_token: 2 * 32 * 128 * 2,
        params_b: 4.0,
        active_params_b: 4.0,
        family: Family::Nemotron,
        tools: true,
        thinking: true,
        cuda_only: false,
        licence_url: "https://www.nvidia.com/en-us/agreements/enterprise-software/nvidia-nemotron-open-model-license/",
        moe_layers: 0,
    },
    ExecModelSpec {
        spec: ModelSpec {
            id: "qwen2.5-7b-instruct",
            label: "Qwen2.5 7B Instruct",
            tier: Tier::Balanced,
            repo: "bartowski/Qwen2.5-7B-Instruct-GGUF",
            file: "Qwen2.5-7B-Instruct-Q4_K_M.gguf",
            size_bytes: 4_683_074_240,
            sha256: "65b8fcd92af6b4fefa935c625d1ac27ea29dcb6ee14589c55a8f115ceaaa1423",
            params: "7.6B · Q4_K_M",
            min_ram_gb: 8,
            licence: "Apache-2.0",
        },
        max_ctx: 32_768,
        // 28 layers · 4 KV heads · head_dim 128. Strong at summaries and minutes in Spanish and English.
        kv_bytes_per_token: 2 * 28 * 4 * 128 * 2,
        params_b: 7.6,
        active_params_b: 7.6,
        family: Family::Qwen,
        tools: true,
        thinking: false,
        cuda_only: false,
        licence_url: "",
        moe_layers: 0,
    },
    ExecModelSpec {
        spec: ModelSpec {
            id: "qwen2.5-14b-instruct",
            label: "Qwen2.5 14B Instruct",
            tier: Tier::Large,
            repo: "bartowski/Qwen2.5-14B-Instruct-GGUF",
            file: "Qwen2.5-14B-Instruct-Q4_K_M.gguf",
            size_bytes: 8_988_110_976,
            sha256: "e47ad95dad6ff848b431053b375adb5d39321290ea2c638682577dafca87c008",
            params: "14.8B · Q4_K_M",
            min_ram_gb: 14,
            licence: "Apache-2.0",
        },
        max_ctx: 32_768,
        // 48 layers · 8 KV heads · head_dim 128.
        kv_bytes_per_token: 2 * 48 * 8 * 128 * 2,
        params_b: 14.8,
        active_params_b: 14.8,
        family: Family::Qwen,
        tools: true,
        thinking: false,
        cuda_only: false,
        licence_url: "",
        moe_layers: 0,
    },
    ExecModelSpec {
        spec: ModelSpec {
            id: "nemotron-cascade-2-30b-a3b",
            label: "Nemotron Cascade 2 30B-A3B",
            tier: Tier::Large,
        // IQ4_XS rather than Q4_K_M (24.7 GB): it fits a 32 GB Mac's Metal working set with room for context.
            repo: "bartowski/nvidia_Nemotron-Cascade-2-30B-A3B-GGUF",
            file: "nvidia_Nemotron-Cascade-2-30B-A3B-IQ4_XS.gguf",
            size_bytes: 18_166_698_432,
            sha256: "871a80bfd682289f2efa1b4fdee899576b5a768681f2b2ad74a15b59af6a510e",
            params: "31.6B (3.6B activos) · IQ4_XS",
            min_ram_gb: 24,
            licence: "NVIDIA-Open-Model-License",
        },
        max_ctx: 262_144,
        // 52 layers, 6 with attention at 2 KV heads × 128: 6 KiB a token. 128 experts, 6 routed + 1 shared.
        kv_bytes_per_token: 2 * 12 * 128 * 2,
        params_b: 31.6,
        active_params_b: 3.6,
        family: Family::Nemotron,
        tools: true,
        thinking: true,
        cuda_only: false,
        licence_url: "https://www.nvidia.com/en-us/agreements/enterprise-software/nvidia-open-model-license/",
        moe_layers: 23,
    },
    ExecModelSpec {
        spec: ModelSpec {
            id: "qwen3-30b-a3b-instruct-2507",
            label: "Qwen3 30B-A3B Instruct",
            tier: Tier::Large,
            repo: "unsloth/Qwen3-30B-A3B-Instruct-2507-GGUF",
            file: "Qwen3-30B-A3B-Instruct-2507-Q4_K_M.gguf",
            size_bytes: 18_556_686_752,
            sha256: "6c997b8af17debdfb01d890214400ccbab00db6acc0ba8da5de1cc906c4774d0",
            params: "30.5B MoE · Q4_K_M",
            min_ram_gb: 24,
            licence: "Apache-2.0",
        },
        max_ctx: 262_144,
        // 48 layers · 4 KV heads · head_dim 128. A mixture: ~3.3B of its 30.5B are read per token.
        kv_bytes_per_token: 2 * 48 * 4 * 128 * 2,
        params_b: 30.5,
        active_params_b: 3.3,
        family: Family::Qwen,
        tools: true,
        thinking: false,
        cuda_only: false,
        licence_url: "",
        moe_layers: 48,
    },
    ExecModelSpec {
        spec: ModelSpec {
            id: "nemotron-3.5-lightning-30b-a3b",
            label: "Nemotron 3.5 Lightning 30B-A3B",
            tier: Tier::Large,
            repo: "ggml-org/NVIDIA-Nemotron-3.5-Lightning-30B-A3B-GGUF",
            file: "NVIDIA-Nemotron-3.5-Lightning-30B-A3B-Q4_0.gguf",
            size_bytes: 18_898_091_584,
            sha256: "61f87e75974e4b535dcdf9aad056541a9514f1dfa4538b463b081d19b7a00e3c",
            params: "31.6B (3.6B activos) · Q4_0",
            min_ram_gb: 24,
            licence: "NVIDIA-Open-Model-License",
        },
        max_ctx: 1_048_576,
        // Same backbone as Nano 30B-A3B: 6 attention layers of 52, 2 KV heads × 128.
        kv_bytes_per_token: 2 * 12 * 128 * 2,
        params_b: 31.6,
        active_params_b: 3.6,
        family: Family::Nemotron,
        tools: true,
        thinking: true,
        cuda_only: false,
        licence_url: "https://www.nvidia.com/en-us/agreements/enterprise-software/nvidia-open-model-license/",
        moe_layers: 23,
    },
    ExecModelSpec {
        spec: ModelSpec {
            id: "nemotron-3.5-bolt-74b-a9b",
            label: "Nemotron 3.5 Bolt 74B-A9B",
            tier: Tier::Large,
        // NVIDIA's 24 GB packing: IQ2_XXS experts and NVFP4 elsewhere, which llama.cpp runs on CUDA only.
            repo: "nvidia/NVIDIA-Nemotron-3.5-Bolt-74B-A9B-24GB-GGUF",
            file: "NVIDIA-Nemotron-3.5-Bolt-74B-A9B-24GB.gguf",
            size_bytes: 22_018_326_528,
            sha256: "91d63363e782c8a3d5f4be42816eba8e0d42dd2530bd07a7d0d2dc440599af75",
            params: "74B (9B activos) · IQ2/NVFP4",
            min_ram_gb: 32,
            licence: "OpenMDW-1.1",
        },
        max_ctx: 262_144,
        // 88 layers, 8 with attention at 2 KV heads × 128: 8 KiB a token.
        kv_bytes_per_token: 2 * 16 * 128 * 2,
        params_b: 74.0,
        active_params_b: 9.0,
        family: Family::Nemotron,
        tools: true,
        thinking: true,
        cuda_only: true,
        licence_url: "https://openmdw.ai/license/1-1/",
        moe_layers: 40,
    },
    ExecModelSpec {
        spec: ModelSpec {
            id: "nemotron-3-nano-30b-a3b",
            label: "Nemotron 3 Nano 30B-A3B",
            tier: Tier::Large,
            repo: "ggml-org/NVIDIA-Nemotron-3-Nano-30B-A3B-GGUF",
            file: "NVIDIA-Nemotron-3-Nano-30B-A3B-Q4_K_M.gguf",
            size_bytes: 22_421_827_488,
            sha256: "0f111a0d49777a2a0178758b1aee14e5365c495987499401ef3ebe87754322a8",
            params: "31.6B (3.6B activos) · Q4_K_M",
            min_ram_gb: 32,
            licence: "NVIDIA-Open-Model-License",
        },
        max_ctx: 1_048_576,
        // 52 layers, 6 with attention at 2 KV heads × 128: 6 KiB a token.
        kv_bytes_per_token: 2 * 12 * 128 * 2,
        params_b: 31.6,
        active_params_b: 3.6,
        family: Family::Nemotron,
        tools: true,
        thinking: true,
        cuda_only: false,
        licence_url: "https://www.nvidia.com/en-us/agreements/enterprise-software/nvidia-open-model-license/",
        moe_layers: 23,
    },
    ExecModelSpec {
        spec: ModelSpec {
            id: "nemotron-3.5-super-120b-a12b",
            label: "Nemotron 3.5 Super 120B-A12B",
            tier: Tier::Large,
            repo: "ggml-org/NVIDIA-Nemotron-3.5-Super-VL-120B-A12B-GGUF",
            file: "NVIDIA-Nemotron-3.5-Super-VL-120B-A12B-Q4_0.gguf",
            size_bytes: 71_933_775_808,
            sha256: "73aabf5646b250c32b0690f3996a0c30f3b50f3d531dcdb03fbafe311139dc83",
            params: "120B (12B activos) · Q4_0",
            min_ram_gb: 96,
            licence: "NVIDIA-Open-Model-License",
        },
        max_ctx: 1_048_576,
        // 88 layers, 8 with attention at 2 KV heads × 128: 8 KiB a token. Text only — the vision
        // projector is a separate file this catalogue does not fetch.
        kv_bytes_per_token: 2 * 16 * 128 * 2,
        params_b: 120.0,
        active_params_b: 12.0,
        family: Family::Nemotron,
        tools: true,
        thinking: true,
        cuda_only: false,
        licence_url: "https://www.nvidia.com/en-us/agreements/enterprise-software/nvidia-open-model-license/",
        moe_layers: 40,
    },
];

impl ExecModelSpec {
    /// The share of the weights read per token — see [`crate::hardware::budget::Shape`].
    pub fn active_share(&self) -> f64 {
        f64::from(self.active_params_b / self.params_b).clamp(0.0, 1.0)
    }
}

/// What a user who has chosen nothing is offered when the machine gives no better answer
/// ([`crate::hardware::budget::recommend`] usually does): small, general, multilingual.
pub const DEFAULT_EXEC_MODEL_ID: &str = "qwen3-4b-instruct-2507";

/// The entry with this id, or `None` for an id from another build's catalogue.
pub fn find(id: &str) -> Option<&'static ExecModelSpec> {
    EXEC_CATALOGUE.iter().find(|entry| entry.spec.id == id)
}

/// The same weights as a catalogue entry, as Ollama's library names them — what the settings pane
/// offers to pull when the server is Ollama, with the entry's memory figures standing for them.
pub struct OllamaTag {
    pub tag: &'static str,
    pub catalogue_id: &'static str,
    /// Every layer of the registry manifest, summed — what the pull downloads. Read from
    /// `registry.ollama.ai/v2/library/<name>/manifests/<tag>` on 2026-10-02.
    pub size_bytes: u64,
    /// The tag's weights are the catalogue file itself (within a few MB). `false` when Ollama
    /// publishes another quantisation of the same model: the entry still stands for its
    /// architecture (window, KV per token, active share) and the tag's own size is what is judged.
    /// Read by the catalogue test that holds same-file tags to the entry's size.
    #[cfg_attr(not(test), allow(dead_code))]
    pub same_file: bool,
}

pub const OLLAMA_TAGS: &[OllamaTag] = &[
    // Read on 2026-10-10, like the two GGUFs they stand for.
    OllamaTag { tag: "phi4-mini:3.8b", catalogue_id: "phi-4-mini-instruct", size_bytes: 2_491_876_363, same_file: true },
    OllamaTag { tag: "qwen3:4b-instruct", catalogue_id: "qwen3-4b-instruct-2507", size_bytes: 2_497_293_316, same_file: true },
    // The three general Qwen tags are sized from the GGUF they stand for, not read from Ollama's registry.
    OllamaTag { tag: "qwen2.5:7b", catalogue_id: "qwen2.5-7b-instruct", size_bytes: 4_683_074_240, same_file: false },
    OllamaTag { tag: "qwen2.5:14b", catalogue_id: "qwen2.5-14b-instruct", size_bytes: 8_988_110_976, same_file: false },
    OllamaTag { tag: "qwen3:30b-a3b-instruct-2507-q4_K_M", catalogue_id: "qwen3-30b-a3b-instruct-2507", size_bytes: 18_556_686_752, same_file: false },
    // Read on 2026-10-19. Ollama's own quantisations of the Nemotron rows above.
    OllamaTag { tag: "nemotron-3-nano:4b", catalogue_id: "nemotron-3-nano-4b", size_bytes: 2_837_597_147, same_file: true },
    OllamaTag { tag: "nemotron-cascade-2:30b", catalogue_id: "nemotron-cascade-2-30b-a3b", size_bytes: 24_272_443_719, same_file: false },
    OllamaTag { tag: "nemotron-3-nano:30b", catalogue_id: "nemotron-3-nano-30b-a3b", size_bytes: 24_271_934_866, same_file: false },
    OllamaTag { tag: "nemotron-3.5-lightning:30b", catalogue_id: "nemotron-3.5-lightning-30b-a3b", size_bytes: 25_430_749_387, same_file: false },
    OllamaTag { tag: "nemotron-3.5-bolt:74b-a9b", catalogue_id: "nemotron-3.5-bolt-74b-a9b", size_bytes: 24_750_264_767, same_file: false },
    OllamaTag { tag: "nemotron-3.5-super:120b", catalogue_id: "nemotron-3.5-super-120b-a12b", size_bytes: 90_153_518_373, same_file: false },
];


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_files_are_unique_and_digests_filled_in() {
        let mut ids = std::collections::HashSet::new();
        let mut files = std::collections::HashSet::new();
        for entry in EXEC_CATALOGUE {
            assert!(ids.insert(entry.spec.id), "duplicate id {}", entry.spec.id);
            assert!(files.insert(entry.spec.file), "duplicate file {}", entry.spec.file);
            assert_eq!(entry.spec.sha256.len(), 64, "{}", entry.spec.id);
            assert!(entry.spec.sha256.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
            assert!(entry.spec.file.ends_with(".gguf") && !entry.spec.file.contains("-of-0"));
            assert!(entry.active_params_b <= entry.params_b);
            assert!(entry.kv_bytes_per_token > 0 && entry.max_ctx >= 8_192);
        }
        assert!(find(DEFAULT_EXEC_MODEL_ID).is_some());
    }

    #[test]
    fn every_ollama_tag_names_an_entry_of_its_size() {
        for tag in OLLAMA_TAGS {
            let entry = find(tag.catalogue_id).unwrap_or_else(|| panic!("{} names no entry", tag.tag));
            if tag.same_file {
                assert!(tag.size_bytes.abs_diff(entry.spec.size_bytes) < 50_000_000, "{}", tag.tag);
            }
        }
    }
}
