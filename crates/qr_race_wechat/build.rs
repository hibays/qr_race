use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=../../models/detect.onnx");
    println!("cargo:rerun-if-changed=../../models/sr.onnx");

    // Android uses st-zrt; only desktop compiles the AOT backend.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        return;
    }

    let out = env::var("OUT_DIR").expect("OUT_DIR");
    generate(
        "detect.onnx",
        "Detect",
        "detect_model.rs",
        "detect_weights.bin",
        &out,
    );
    generate("sr.onnx", "Sr", "sr_model.rs", "sr_weights.bin", &out);
}

fn generate(model_file: &str, name: &str, code_file: &str, weights_file: &str, out: &str) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let model_path = root.join("models").join(model_file);
    let mut model = lele::model::OnnxModel::load(&model_path)
        .unwrap_or_else(|error| panic!("lele: load {}: {error}", model_path.display()));
    let graph = model
        .proto
        .graph
        .as_mut()
        .unwrap_or_else(|| panic!("lele: {} has no graph", model_file));

    // lele sanitizes `/`, `:` and `.` when naming generated variables. This
    // model has both `data/bn/scale:bias` (initializer) and
    // `data/bn/scale/bias` (node output), which collapse to the same Rust name
    // and make the generated Conv read the 1x1 bias tensor as its input.
    let mut renamed = std::collections::HashMap::new();
    for (index, initializer) in graph.initializer.iter_mut().enumerate() {
        let new_name = format!("init_{index}");
        renamed.insert(initializer.name.clone(), new_name.clone());
        initializer.name = new_name;
    }
    for node in &mut graph.node {
        for input in &mut node.input {
            if let Some(new_name) = renamed.get(input) {
                *input = new_name.clone();
            }
        }
    }

    // The exported graph computes mbox_conf_flatten as
    // Flatten(Softmax(Reshape(mbox_conf))). lele 0.1.x cannot lower that
    // dynamic reshape; expose the pre-softmax Concat directly and apply the
    // 2-class softmax in Rust.
    if model_file == "detect.onnx" {
        graph.node.retain(|node| {
            !node.output.iter().any(|output| {
                matches!(
                    output.as_str(),
                    "mbox_conf_reshape" | "mbox_conf_softmax" | "mbox_conf_flatten"
                )
            })
        });
        for node in &mut graph.node {
            for output in &mut node.output {
                if output == "mbox_conf" {
                    *output = "mbox_conf_flatten".to_string();
                }
            }
        }
    }

    // Keep only the outputs the scanner consumes. The detector's priorbox
    // branch is unused by SSDDetector.
    let keep: &[&str] = if model_file == "detect.onnx" {
        &["mbox_loc", "mbox_conf_flatten"]
    } else {
        &["fc"]
    };
    prune_graph(graph, keep);

    let compiler = lele::compiler::Compiler::new()
        .with_name(name)
        .with_default_optimizations()
        .with_constant_folding(true);
    let result = compiler
        .compile(&*graph)
        .unwrap_or_else(|error| panic!("lele: compile {model_file}: {error}"));
    let code = result.code;
    // `include!` cannot carry the generated inner crate attributes.
    let code = code
        .lines()
        .filter(|line| !line.starts_with("#!["))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(Path::new(out).join(code_file), code).expect("write generated code");
    fs::write(Path::new(out).join(weights_file), result.weights).expect("write generated weights");
}

fn prune_graph(graph: &mut lele::model::onnx_proto::GraphProto, keep: &[&str]) {
    use std::collections::{HashMap, HashSet};

    let keep: HashSet<&str> = keep.iter().copied().collect();
    let mut producers: HashMap<String, usize> = HashMap::new();
    for (index, node) in graph.node.iter().enumerate() {
        for output in &node.output {
            producers.insert(output.clone(), index);
        }
    }

    let mut needed: HashSet<usize> = HashSet::new();
    let mut stack: Vec<String> = graph
        .output
        .iter()
        .filter(|output| keep.contains(output.name.as_str()))
        .map(|output| output.name.clone())
        .collect();
    while let Some(name) = stack.pop() {
        if let Some(&index) = producers.get(&name)
            && needed.insert(index)
        {
            for input in &graph.node[index].input {
                stack.push(input.clone());
            }
        }
    }

    let mut index = 0;
    graph.node.retain(|_| {
        let keep_node = needed.contains(&index);
        index += 1;
        keep_node
    });
    graph
        .output
        .retain(|output| keep.contains(output.name.as_str()));
}
