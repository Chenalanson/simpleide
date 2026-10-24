//! Safe synchronization of edited build settings into CubeMX-style Makefiles.
//!
//! The editor never rewrites arbitrary Makefiles. Only files that expose the
//! usual CubeMX variables (`C_DEFS`, `C_INCLUDES`, `OPT` and a `CFLAGS` line
//! using them) are touched. Every write is backed up in the application data
//! directory, and line endings/BOM are preserved.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::BuildOptions;
pub(crate) use crate::project_meta::find_makefile;
use crate::util::{read_text_file, write_text_file};

const OPTIONS_START: &str = "# >>> Simple embedded managed options (auto-generated) >>>";
const OPTIONS_END: &str = "# <<< Simple embedded managed options <<<";
const OUTPUT_START: &str = "# >>> Simple embedded managed outputs (auto-generated) >>>";
const OUTPUT_END: &str = "# <<< Simple embedded managed outputs <<<";
#[cfg(not(test))]
const MAX_BACKUPS: usize = 12;

#[derive(Clone, Debug)]
struct Assignment {
    start: usize,
    end: usize,
    operator: String,
    value: String,
}

/// Parse the options that are currently visible in the Makefile.
///
/// Missing values keep the same defaults as [`BuildOptions`]. The UI compares
/// the parsed values with the edited values and only rewrites what changed.
pub fn read_options(makefile: &Path, fallback: &BuildOptions) -> Option<BuildOptions> {
    let loaded = read_text_file(makefile).ok()?;
    let lines = text_lines(&loaded.text);
    if !is_supported(&lines) {
        return None;
    }
    Some(parse_options(&lines, fallback))
}

/// Rewrite the include/library/define variables with the given values.
pub fn sync_lists(
    root: &Path,
    includes: &[String],
    library_dirs: &[String],
    defines: &[String],
) -> Result<Option<PathBuf>, String> {
    let Some(makefile) = find_makefile(root) else {
        return Ok(None);
    };
    let loaded = read_text_file(&makefile)?;
    let mut lines = text_lines(&loaded.text);
    if !is_supported(&lines) {
        return Err("鏈瘑鍒殑 Makefile 缁撴瀯锛屾湭鑷姩淇敼".to_owned());
    }

    let target_includes = clean_list(includes, "-I");
    let target_library_dirs = clean_list(library_dirs, "-L");
    let target_defines = clean_list(defines, "-D");
    if current_list(&lines, "C_INCLUDES", "-I") == target_includes
        && current_list(&lines, "LIBDIR", "-L") == target_library_dirs
        && current_list(&lines, "C_DEFS", "-D") == target_defines
    {
        return Ok(None);
    }

    replace_assignment(
        &mut lines,
        "C_INCLUDES",
        &list_assignment("C_INCLUDES", "-I", &target_includes),
    )?;
    replace_assignment(
        &mut lines,
        "LIBDIR",
        &list_assignment("LIBDIR", "-L", &target_library_dirs),
    )?;
    replace_assignment(
        &mut lines,
        "C_DEFS",
        &list_assignment("C_DEFS", "-D", &target_defines),
    )?;

    commit(&makefile, root, &loaded, lines)
}

/// Rewrite only the build-option fields that differ between `before` and `after`.
pub fn sync_options(
    root: &Path,
    before: &BuildOptions,
    after: &BuildOptions,
) -> Result<Option<PathBuf>, String> {
    let Some(makefile) = find_makefile(root) else {
        return Ok(None);
    };
    let loaded = read_text_file(&makefile)?;
    let mut lines = text_lines(&loaded.text);
    if !is_supported(&lines) {
        return Err("鏈瘑鍒殑 Makefile 缁撴瀯锛屾湭鑷姩淇敼".to_owned());
    }

    if before.optimization != after.optimization && !after.optimization.trim().is_empty() {
        replace_assignment(
            &mut lines,
            "OPT",
            &[format!("OPT = {}", after.optimization.trim())],
        )?;
    }

    let cflags = main_cflags(&lines)
        .ok_or_else(|| "Makefile 涓湭鎵惧埌 CFLAGS 涓荤紪璇戣锛屾湭鑷姩淇敼".to_owned())?;
    let cflags_tokens =
        update_compile_flags(&tokens(&cflags.value), before, after, CompileFlags::C);
    if cflags_tokens != tokens(&cflags.value) {
        replace_span(
            &mut lines,
            cflags.start,
            cflags.end,
            &[format!(
                "CFLAGS {} {}",
                cflags.operator,
                cflags_tokens.join(" ")
            )],
        );
    }

    if let Some(asflags) = first_assignment(&lines, "ASFLAGS") {
        let asflags_tokens = update_compile_flags(
            &tokens(&asflags.value),
            before,
            after,
            CompileFlags::Assembler,
        );
        if asflags_tokens != tokens(&asflags.value) {
            replace_span(
                &mut lines,
                asflags.start,
                asflags.end,
                &[format!(
                    "ASFLAGS {} {}",
                    asflags.operator,
                    asflags_tokens.join(" ")
                )],
            );
        }
    }

    if before.assembler_defines != after.assembler_defines {
        replace_assignment(
            &mut lines,
            "AS_DEFS",
            &list_assignment("AS_DEFS", "-D", &after.assembler_defines),
        )?;
    }

    if managed_options_changed(before, after) {
        let existing = managed_block(&lines, OPTIONS_START, OPTIONS_END);
        let footer = managed_footer(before, after, &existing);
        replace_managed_block(&mut lines, OPTIONS_START, OPTIONS_END, &footer);
    }

    if before.linker_output_format != after.linker_output_format {
        let outputs = managed_outputs(&lines, after)?;
        replace_managed_block(&mut lines, OUTPUT_START, OUTPUT_END, &outputs);
    }

    commit(&makefile, root, &loaded, lines)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CompileFlags {
    C,
    Assembler,
}

fn update_compile_flags(
    current: &[String],
    before: &BuildOptions,
    after: &BuildOptions,
    target: CompileFlags,
) -> Vec<String> {
    let mut flags = current.to_vec();

    if target == CompileFlags::C && before.c_standard != after.c_standard {
        flags.retain(|flag| !flag.starts_with("-std="));
        let standard = after.c_standard.trim();
        if !standard.is_empty() {
            flags.push(format!("-std={standard}"));
        }
    }

    if before.warning != after.warning {
        flags.retain(|flag| !matches!(flag.as_str(), "-Wall" | "-Wextra" | "-w"));
        let warning = after.warning.trim();
        if !warning.is_empty() && warning != "ac5-like" {
            flags.extend(tokens(warning));
        }
    }

    if before.lto != after.lto && target == CompileFlags::C {
        flags.retain(|flag| !matches!(flag.as_str(), "-flto" | "-fno-lto"));
        if after.lto {
            flags.push("-flto".to_owned());
        }
    }

    if before.function_sections != after.function_sections {
        flags.retain(|flag| !matches!(flag.as_str(), "-fdata-sections" | "-ffunction-sections"));
        if after.function_sections {
            flags.push("-fdata-sections".to_owned());
            flags.push("-ffunction-sections".to_owned());
        }
    }

    if target == CompileFlags::C {
        if before.plain_char_signed != after.plain_char_signed {
            flags.retain(|flag| !matches!(flag.as_str(), "-fsigned-char" | "-funsigned-char"));
            if after.plain_char_signed {
                flags.push("-fsigned-char".to_owned());
            }
        }

        if before.short_enums != after.short_enums {
            flags.retain(|flag| flag != "-fshort-enums");
            if after.short_enums {
                flags.push("-fshort-enums".to_owned());
            }
        }
    }

    if before.big_endian != after.big_endian {
        flags.retain(|flag| !matches!(flag.as_str(), "-mbig-endian" | "-mlittle-endian"));
        if after.big_endian {
            flags.push("-mbig-endian".to_owned());
        }
    }

    dedupe(flags)
}

fn managed_outputs(lines: &[String], after: &BuildOptions) -> Result<Vec<String>, String> {
    if after.linker_output_format != "AXF" {
        return Ok(Vec::new());
    }
    if lines.iter().any(|line| line.contains("$(TARGET).axf:")) {
        return Ok(managed_block(lines, OUTPUT_START, OUTPUT_END));
    }
    if !lines.iter().any(|line| line.contains("$(TARGET).elf")) {
        return Err("Makefile 未使用 $(TARGET).elf，无法生成 .axf".to_owned());
    }
    let copy = if first_assignment(lines, "CP").is_some() {
        "$(CP)"
    } else {
        "cp"
    };
    Ok(vec![
        "all: $(BUILD_DIR)/$(TARGET).axf".to_owned(),
        String::new(),
        "$(BUILD_DIR)/$(TARGET).axf: $(BUILD_DIR)/$(TARGET).elf".to_owned(),
        format!("\t{copy} $< $@"),
    ])
}

fn managed_options_changed(before: &BuildOptions, after: &BuildOptions) -> bool {
    before.debug_info != after.debug_info
        || before.cpp_standard != after.cpp_standard
        || before.rtti != after.rtti
        || before.lto != after.lto
        || before.linker_options != after.linker_options
        || before.assembler_options != after.assembler_options
}

fn managed_footer(before: &BuildOptions, after: &BuildOptions, existing: &[String]) -> Vec<String> {
    let mut lines = Vec::new();
    let has_line = |predicate: &dyn Fn(&str) -> bool| existing.iter().any(|line| predicate(line));

    if before.debug_info != after.debug_info
        || has_line(&|line| matches!(line, "CFLAGS += -g" | "CFLAGS += -g0"))
    {
        lines.push(format!(
            "CFLAGS += {}",
            if after.debug_info == "disable" {
                "-g0"
            } else {
                "-g"
            }
        ));
    }

    if (before.cpp_standard != after.cpp_standard
        || has_line(&|line| line.starts_with("CXXFLAGS += -std=")))
        && !after.cpp_standard.trim().is_empty()
    {
        lines.push(format!("CXXFLAGS += -std={}", after.cpp_standard.trim()));
    }

    if before.rtti != after.rtti
        || has_line(&|line| matches!(line, "CXXFLAGS += -frtti" | "CXXFLAGS += -fno-rtti"))
    {
        lines.push(format!(
            "CXXFLAGS += {}",
            if after.rtti { "-frtti" } else { "-fno-rtti" }
        ));
    }

    if before.lto != after.lto
        || has_line(&|line| matches!(line, "LDFLAGS += -flto" | "LDFLAGS += -fno-lto"))
    {
        lines.push(format!(
            "LDFLAGS += {}",
            if after.lto { "-flto" } else { "-fno-lto" }
        ));
    }

    if (before.linker_options != after.linker_options
        || has_line(&|line| {
            line.starts_with("LDFLAGS +=")
                && !matches!(line, "LDFLAGS += -flto" | "LDFLAGS += -fno-lto")
        }))
        && !after.linker_options.trim().is_empty()
    {
        lines.push(format!("LDFLAGS += {}", after.linker_options.trim()));
    }

    if (before.assembler_options != after.assembler_options
        || has_line(&|line| line.starts_with("ASFLAGS +=")))
        && !after.assembler_options.trim().is_empty()
    {
        lines.push(format!("ASFLAGS += {}", after.assembler_options.trim()));
    }

    lines
}

fn parse_options(lines: &[String], fallback: &BuildOptions) -> BuildOptions {
    let mut options = BuildOptions::default();
    let cflags = all_values(lines, "CFLAGS").join(" ");
    let cxxflags = all_values(lines, "CXXFLAGS").join(" ");
    let linker = all_values(lines, "LDFLAGS").join(" ");

    let opt_flag = first_assignment(lines, "OPT").and_then(|opt| {
        tokens(&opt.value)
            .into_iter()
            .find(|flag| flag.starts_with("-O"))
    });
    let optimization = opt_flag.or_else(|| {
        tokens(&cflags)
            .into_iter()
            .find(|flag| flag.starts_with("-O"))
    });
    if let Some(flag) = optimization {
        options.optimization = flag;
    }

    if let Some(standard) = find_prefix(&cflags, "-std=") {
        options.c_standard = standard;
    }
    if let Some(standard) = find_prefix(&cxxflags, "-std=") {
        options.cpp_standard = standard;
    }

    let warnings = ["-Wall", "-Wextra", "-w"]
        .into_iter()
        .filter(|warning| has_flag(&cflags, warning))
        .collect::<Vec<_>>();
    options.warning = warnings.join(" ");

    options.lto = has_flag(&cflags, "-flto") || has_flag(&linker, "-flto");
    options.function_sections =
        has_flag(&cflags, "-fdata-sections") || has_flag(&cflags, "-ffunction-sections");
    options.plain_char_signed = has_flag(&cflags, "-fsigned-char");
    options.short_enums = has_flag(&cflags, "-fshort-enums");
    options.rtti = has_flag(&cxxflags, "-frtti")
        || (!has_flag(&cxxflags, "-fno-rtti") && has_flag(&cflags, "-frtti"));
    options.big_endian = has_flag(&cflags, "-mbig-endian");
    options.debug_info = if has_flag(&cflags, "-g") || cflags.contains("-gdwarf") {
        "enable".to_owned()
    } else {
        "disable".to_owned()
    };

    if let Some(as_defs) = first_assignment(lines, "AS_DEFS") {
        options.assembler_defines = tokens(&as_defs.value)
            .into_iter()
            .filter_map(|flag| flag.strip_prefix("-D").map(ToOwned::to_owned))
            .filter(|value| !value.is_empty())
            .collect();
    }

    options.linker_output_format = if lines.iter().any(|line| line.contains("$(TARGET).axf:")) {
        "AXF".to_owned()
    } else {
        "ELF".to_owned()
    };

    // Values written by previous syncs live in a small managed footer. Read
    // them back so that clearing a field also removes the generated line.
    for line in managed_block(lines, OPTIONS_START, OPTIONS_END) {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("LDFLAGS +=") {
            options.linker_options = value.trim().to_owned();
        } else if let Some(value) = trimmed.strip_prefix("ASFLAGS +=") {
            options.assembler_options = value.trim().to_owned();
        } else if trimmed == "CFLAGS += -g0" {
            options.debug_info = "disable".to_owned();
        } else if trimmed == "CFLAGS += -g" {
            options.debug_info = "enable".to_owned();
        } else if let Some(value) = trimmed.strip_prefix("CXXFLAGS += -std=") {
            options.cpp_standard = value.trim().to_owned();
        } else if trimmed == "CXXFLAGS += -frtti" {
            options.rtti = true;
        } else if trimmed == "CXXFLAGS += -fno-rtti" {
            options.rtti = false;
        }
    }

    // These fields have no reliable GCC/Makefile equivalent; keep whatever the
    // user saved in the application settings instead of resetting them.
    options.use_microlib = fallback.use_microlib;
    options.split_load_store = fallback.split_load_store;
    options.assembler_type = fallback.assembler_type.clone();
    options.linker_no_binary = fallback.linker_no_binary;
    options.linker_binary_excludes = fallback.linker_binary_excludes.clone();
    options.linker_xo_base = fallback.linker_xo_base.clone();
    options.linker_ro_base = fallback.linker_ro_base.clone();
    options.linker_rw_base = fallback.linker_rw_base.clone();

    options
}

fn is_supported(lines: &[String]) -> bool {
    first_assignment(lines, "C_DEFS").is_some()
        && first_assignment(lines, "C_INCLUDES").is_some()
        && first_assignment(lines, "OPT").is_some()
        && main_cflags(lines).is_some_and(|cflags| {
            cflags.value.contains("$(C_DEFS)") && cflags.value.contains("$(C_INCLUDES)")
        })
}

fn main_cflags(lines: &[String]) -> Option<Assignment> {
    assignments(lines, "CFLAGS")
        .into_iter()
        .find(|cflags| cflags.value.contains("$(C_DEFS)"))
}

fn assignments(lines: &[String], key: &str) -> Vec<Assignment> {
    let mut result = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let Some(mut assignment) = assignment_at(lines, index, key) else {
            index += 1;
            continue;
        };
        assignment.end = assignment_end(lines, index);
        assignment.value = joined_value(lines, index, assignment.end, key, &assignment.operator);
        result.push(assignment);
        index += 1;
    }
    result
}

fn first_assignment(lines: &[String], key: &str) -> Option<Assignment> {
    assignments(lines, key).into_iter().next()
}

fn assignment_at(lines: &[String], index: usize, key: &str) -> Option<Assignment> {
    let trimmed = lines[index].trim_start();
    if trimmed.starts_with('#') {
        return None;
    }
    let rest = trimmed.strip_prefix(key)?;
    let rest = rest.trim_start();
    let operator = if rest.starts_with(":=") {
        ":="
    } else if rest.starts_with("+=") {
        "+="
    } else if rest.starts_with("?=") {
        "?="
    } else if rest.starts_with('=') {
        "="
    } else {
        return None;
    };
    Some(Assignment {
        start: index,
        end: index + 1,
        operator: operator.to_owned(),
        value: String::new(),
    })
}

fn assignment_end(lines: &[String], start: usize) -> usize {
    let mut end = start + 1;
    let mut current = lines[start].trim_end();
    while current.ends_with('\\') && end < lines.len() {
        current = lines[end].trim_end();
        end += 1;
    }
    end
}

fn joined_value(lines: &[String], start: usize, end: usize, key: &str, operator: &str) -> String {
    let trimmed = lines[start].trim_start();
    let rest = trimmed.strip_prefix(key).unwrap_or(trimmed).trim_start();
    let first = rest.strip_prefix(operator).unwrap_or(rest).trim();
    let mut value = first.trim_end_matches('\\').trim().to_owned();
    for line in &lines[start + 1..end] {
        let line = line.trim().trim_end_matches('\\').trim();
        if line.is_empty() {
            continue;
        }
        if !value.is_empty() {
            value.push(' ');
        }
        value.push_str(line);
    }
    value
}

fn replace_assignment(
    lines: &mut Vec<String>,
    key: &str,
    replacement: &[String],
) -> Result<(), String> {
    let Some(assignment) = first_assignment(lines, key) else {
        return Err(format!("Makefile 涓湭鎵惧埌 {key}"));
    };
    replace_span(lines, assignment.start, assignment.end, replacement);
    Ok(())
}

fn replace_span(lines: &mut Vec<String>, start: usize, end: usize, replacement: &[String]) {
    lines.splice(start..end, replacement.iter().cloned());
}

fn clean_list(values: &[String], prefix: &str) -> Vec<String> {
    dedupe(
        values
            .iter()
            .map(|value| value.trim().trim_start_matches(prefix).trim().to_owned())
            .filter(|value| !value.is_empty())
            .collect(),
    )
}

fn current_list(lines: &[String], key: &str, prefix: &str) -> Vec<String> {
    first_assignment(lines, key)
        .map(|assignment| {
            tokens(&assignment.value)
                .into_iter()
                .filter_map(|item| item.strip_prefix(prefix).map(ToOwned::to_owned))
                .filter(|value| !value.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn list_assignment(key: &str, prefix: &str, values: &[String]) -> Vec<String> {
    if values.is_empty() {
        return vec![format!("{key} =")];
    }
    if values.len() == 1 {
        return vec![format!("{key} = {prefix}{}", values[0])];
    }
    let mut lines = vec![format!("{key} =  \\")];
    for (index, value) in values.iter().enumerate() {
        if index + 1 == values.len() {
            lines.push(format!("{prefix}{value}"));
        } else {
            lines.push(format!("{prefix}{value} \\"));
        }
    }
    lines
}

fn managed_block(lines: &[String], start_marker: &str, end_marker: &str) -> Vec<String> {
    let Some(start) = lines.iter().position(|line| line.as_str() == start_marker) else {
        return Vec::new();
    };
    let end = lines
        .iter()
        .position(|line| line.as_str() == end_marker)
        .unwrap_or(lines.len().saturating_sub(1));
    lines[start + 1..end.min(lines.len())].to_vec()
}

fn replace_managed_block(
    lines: &mut Vec<String>,
    start_marker: &str,
    end_marker: &str,
    block: &[String],
) {
    let start = lines.iter().position(|line| line.as_str() == start_marker);
    if let Some(mut start) = start {
        let end = lines
            .iter()
            .position(|line| line.as_str() == end_marker)
            .unwrap_or(lines.len().saturating_sub(1));
        let end = (end + 1).min(lines.len());
        if start > 0
            && lines[start - 1].trim().is_empty()
            && (end >= lines.len() || lines[end].trim().is_empty())
        {
            start -= 1;
        }
        lines.drain(start..end);
    }

    if block.is_empty() {
        return;
    }
    if lines.last().is_some_and(|line| !line.trim().is_empty()) {
        lines.push(String::new());
    }
    lines.push(start_marker.to_owned());
    lines.extend(block.iter().cloned());
    lines.push(end_marker.to_owned());
}

fn commit(
    path: &Path,
    root: &Path,
    loaded: &crate::util::LoadedText,
    lines: Vec<String>,
) -> Result<Option<PathBuf>, String> {
    let mut text = lines.join("\n");
    if loaded.text.ends_with('\n') {
        text.push('\n');
    }
    if text == loaded.text {
        return Ok(None);
    }
    #[cfg(not(test))]
    backup(path, root)?;
    #[cfg(test)]
    let _ = root;
    write_text_file(path, &text, loaded.line_ending, loaded.bom)?;
    Ok(Some(path.to_path_buf()))
}

#[cfg(not(test))]
fn backup(path: &Path, root: &Path) -> Result<PathBuf, String> {
    let settings = crate::config::settings_path();
    let base = settings
        .parent()
        .ok_or_else(|| "鏃犳硶瀹氫綅璁剧疆鐩綍".to_owned())?;
    let key = crate::util::project_key(root);
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in key.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let name = root
        .file_name()
        .map(|name| sanitize(&name.to_string_lossy()))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "project".to_owned());
    let dir = base
        .join("backups")
        .join(format!("{name}-{:08x}", hash as u32));
    std::fs::create_dir_all(&dir).map_err(|err| format!("无法创建备份目录：{err}"))?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let backup = dir.join(format!("Makefile.{stamp}.bak"));
    std::fs::copy(path, &backup).map_err(|err| format!("无法备份 Makefile：{err}"))?;
    prune_backups(&dir);
    Ok(backup)
}

#[cfg(not(test))]
fn prune_backups(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut backups = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("Makefile.") && name.ends_with(".bak"))
        })
        .collect::<Vec<_>>();
    backups.sort();
    let remove_count = backups.len().saturating_sub(MAX_BACKUPS);
    for path in backups.into_iter().take(remove_count) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(not(test))]
fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn text_lines(text: &str) -> Vec<String> {
    text.lines().map(ToOwned::to_owned).collect()
}

fn all_values(lines: &[String], key: &str) -> Vec<String> {
    assignments(lines, key)
        .into_iter()
        .map(|assignment| assignment.value)
        .collect()
}

fn tokens(value: &str) -> Vec<String> {
    value.split_whitespace().map(ToOwned::to_owned).collect()
}

fn has_flag(value: &str, flag: &str) -> bool {
    value.split_whitespace().any(|item| item == flag)
}

fn find_prefix(value: &str, prefix: &str) -> Option<String> {
    value
        .split_whitespace()
        .find_map(|item| item.strip_prefix(prefix))
        .map(ToOwned::to_owned)
}

fn dedupe(values: Vec<String>) -> Vec<String> {
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        if !result.contains(&value) {
            result.push(value);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::LineEnding;

    fn sample() -> String {
        [
            "TARGET = demo",
            "OPT = -Og",
            "",
            "AS_DEFS = ",
            "C_DEFS =  \\",
            "-DUSE_HAL_DRIVER \\",
            "-DSTM32F103xB",
            "",
            "AS_INCLUDES = ",
            "C_INCLUDES =  \\",
            "-ICore/Inc \\",
            "-IDrivers/CMSIS/Include",
            "",
            "ASFLAGS = $(MCU) $(AS_DEFS) $(AS_INCLUDES) $(OPT) -Wall -fdata-sections -ffunction-sections",
            "CFLAGS += $(MCU) $(C_DEFS) $(C_INCLUDES) $(OPT) -Wall -fdata-sections -ffunction-sections",
            "LDFLAGS = $(MCU) -specs=nano.specs -T$(LDSCRIPT) $(LIBDIR) $(LIBS)",
            "LIBDIR = ",
            "CP = $(PREFIX)objcopy",
            "",
            "$(BUILD_DIR)/$(TARGET).elf: $(OBJECTS) Makefile",
            "\t$(CC) $(OBJECTS) $(LDFLAGS) -o $@",
            "all: $(BUILD_DIR)/$(TARGET).elf $(BUILD_DIR)/$(TARGET).hex",
            "",
        ]
        .join("\n")
    }

    fn temp_project(name: &str, text: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "simple-embedded-makefile-test-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("Makefile");
        write_text_file(&path, text, LineEnding::Lf, false).unwrap();
        (root, path)
    }

    #[test]
    fn reads_existing_options() {
        let (root, path) = temp_project("read", &sample());
        let options = read_options(&path, &BuildOptions::default()).unwrap();
        assert_eq!(options.optimization, "-Og");
        assert_eq!(options.warning, "-Wall");
        assert!(options.function_sections);
        assert_eq!(options.linker_output_format, "ELF");
        assert_eq!(options.assembler_defines, Vec::<String>::new());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn updates_lists_and_preserves_unrelated_lines() {
        let (root, path) = temp_project("lists", &sample());
        let includes = vec!["Core/Inc".to_owned(), "App/Inc".to_owned()];
        let libs = vec!["lib".to_owned()];
        let defines = vec!["APP".to_owned()];
        sync_lists(&root, &includes, &libs, &defines).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("C_INCLUDES =  \\\n-ICore/Inc \\\n-IApp/Inc"));
        assert!(text.contains("LIBDIR = -Llib"));
        assert!(text.contains("C_DEFS = -DAPP"));
        assert!(text.contains("TARGET = demo"));
        assert!(text.contains("LDFLAGS = $(MCU)"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn syncs_only_changed_flags_and_is_idempotent() {
        let (root, path) = temp_project("options", &sample());
        let before = read_options(&path, &BuildOptions::default()).unwrap();
        let mut after = before.clone();
        after.optimization = "-O2".to_owned();
        after.warning = "-Wall -Wextra".to_owned();
        after.function_sections = false;
        after.linker_options = "-Wl,--print-memory-usage".to_owned();

        let first = sync_options(&root, &before, &after).unwrap();
        assert!(first.is_some());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("OPT = -O2"));
        assert!(text.contains("CFLAGS += $(MCU) $(C_DEFS) $(C_INCLUDES) $(OPT) -Wall -Wextra"));
        assert!(!text.contains("CFLAGS += CFLAGS"));
        assert!(!text.contains("-fdata-sections"));
        assert!(text.contains("LDFLAGS += -Wl,--print-memory-usage"));
        assert!(text.contains(OPTIONS_START));

        let parsed = read_options(&path, &BuildOptions::default()).unwrap();
        assert_eq!(parsed.optimization, "-O2");
        assert_eq!(parsed.warning, "-Wall -Wextra");
        assert!(!parsed.function_sections);
        assert_eq!(parsed.linker_options, "-Wl,--print-memory-usage");
        let second = sync_options(&root, &parsed, &after).unwrap();
        assert!(second.is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn adds_and_removes_axf_output() {
        let (root, path) = temp_project("axf", &sample());
        let before = read_options(&path, &BuildOptions::default()).unwrap();
        assert_eq!(before.linker_output_format, "ELF");

        let mut axf = before.clone();
        axf.linker_output_format = "AXF".to_owned();
        sync_options(&root, &before, &axf).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("all: $(BUILD_DIR)/$(TARGET).axf"));
        assert!(text.contains("$(BUILD_DIR)/$(TARGET).axf: $(BUILD_DIR)/$(TARGET).elf"));
        assert!(text.contains("\t$(CP) $< $@"));

        let parsed = read_options(&path, &BuildOptions::default()).unwrap();
        assert_eq!(parsed.linker_output_format, "AXF");
        let mut elf = parsed.clone();
        elf.linker_output_format = "ELF".to_owned();
        sync_options(&root, &parsed, &elf).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("$(TARGET).axf"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_unrelated_makefiles() {
        let (root, path) = temp_project("unrelated", "all:\n\techo hello\n");
        let result = sync_lists(&root, &["inc".to_owned()], &[], &[]);
        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "all:\n\techo hello\n"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn preserves_crlf() {
        let (root, path) = temp_project("crlf", &sample().replace('\n', "\r\n"));
        let loaded = read_text_file(&path).unwrap();
        assert_eq!(loaded.line_ending, LineEnding::Crlf);
        sync_lists(&root, &["Core/Inc".to_owned()], &[], &[]).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.windows(2).any(|pair| pair == b"\r\n"));
        let _ = std::fs::remove_dir_all(root);
    }
}
