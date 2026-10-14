use crate::{failure, load, save_project, Result};
use clap::Subcommand;
use serde_json::Value;

#[derive(Subcommand)]
pub enum LayerCommand {
    /// Move a node to a zero-based index (back to front) in the node array.
    Reorder {
        input: String,
        #[arg(long, alias = "nodes", alias = "id")]
        node: String,
        #[arg(long)]
        index: usize,
        #[arg(long)]
        output: String,
    },
    /// Hide a node; --hidden false shows it again.
    Hide {
        input: String,
        #[arg(long, alias = "nodes", alias = "id")]
        node: String,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        hidden: bool,
        #[arg(long)]
        output: String,
    },
    /// Set editor locking metadata; --locked false unlocks it.
    Lock {
        input: String,
        #[arg(long, alias = "nodes", alias = "id")]
        node: String,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        locked: bool,
        #[arg(long)]
        output: String,
    },
}

fn index(project: &forma_document::Project, id: &str) -> Result<usize> {
    project
        .nodes
        .iter()
        .position(|node| node.id == id)
        .ok_or_else(|| {
            crate::assist::not_found("node", id, project.nodes.iter().map(|n| n.id.as_str()))
        })
}

pub fn convert(input: &str, node: &str, output: &str, back: bool) -> Result<Value> {
    let mut project = load(input)?;
    let index = index(&project, node)?;
    let result = if back {
        project.nodes[index].convert_back()
    } else if matches!(
        project.nodes[index].kind,
        forma_document::NodeKind::Text { .. }
    ) {
        // Text uses local system fonts for its outline.
        project.convert_text_to_path(node, forma_document::system_fonts())
    } else {
        project.nodes[index].convert_to_path()
    };
    result.map_err(|e| failure("conversion_error", e))?;
    save_project(&project, output)
}

pub fn run(command: LayerCommand) -> Result<Value> {
    match command {
        LayerCommand::Reorder {
            input,
            node,
            index: destination,
            output,
        } => {
            let mut project = load(&input)?;
            let source = index(&project, &node)?;
            if destination >= project.nodes.len() {
                return Err(failure(
                    "invalid_arguments",
                    "Layer index must be within the node array",
                ));
            }
            let node = project.nodes.remove(source);
            project.nodes.insert(destination, node);
            save_project(&project, &output)
        }
        LayerCommand::Hide {
            input,
            node,
            hidden,
            output,
        } => {
            let mut project = load(&input)?;
            let index = index(&project, &node)?;
            project.nodes[index].visible = !hidden;
            save_project(&project, &output)
        }
        LayerCommand::Lock {
            input,
            node,
            locked,
            output,
        } => {
            let mut project = load(&input)?;
            let index = index(&project, &node)?;
            project.nodes[index].locked = locked;
            save_project(&project, &output)
        }
    }
}
