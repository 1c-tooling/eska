#!/usr/bin/env python3
"""Read native managed-form shapes without exporting names, UUIDs or source paths."""

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET

FORM_NS = "http://v8.1c.ru/8.3/xcf/logform"
FORM = "{" + FORM_NS + "}"
DATA_KINDS = {"Attribute", "Column", "Command"}


def local(tag):
    """Namespace identity is checked separately from native tag spelling."""
    return tag.rsplit("}", 1)[-1]


def data_path_shape(value, attributes):
    """Classify syntax only; this inventory does not resolve metadata or runtime values."""
    head = value.split(".", 1)[0].lower()
    if head in attributes:
        return "attribute_head"
    if head == "items":
        return "element_head"
    if head.startswith("~"):
        return "marked_head"
    if re.fullmatch(r"[^\[\]]+\[\d+\]", head):
        return "indexed_head"
    if head.startswith("1/0:"):
        return "native_identity_path"
    return "unclassified"


def collect(root):
    """Count real ID scopes, fixed slots and references while verifying unchanged input bytes."""
    result = {
        "schema_version": 1,
        "forms": 0,
        "root_profiles": Counter(),
        "root_sections": Counter(),
        "id_kinds": Counter(),
        "regular_element_kinds": Counter(),
        "fixed_element_kinds": Counter(),
        "fixed_parent_kind_pairs": Counter(),
        "regular_parent_kind_pairs": Counter(),
        "namespace_id_collisions": Counter(),
        "namespace_name_collisions": Counter(),
        "forms_with_id_scope_overlap": Counter(),
        "negative_ids": Counter(),
        "empty_names_by_kind": Counter(),
        "column_containers": 0,
        "column_container_id_collisions": 0,
        "column_container_name_collisions": 0,
        "forms_with_repeated_column_ids_across_containers": 0,
        "attributes_with_repeated_column_ids_across_containers": 0,
        "references": Counter(),
        "unresolved_known_references": Counter(),
        "data_path_shapes": Counter(),
        "title_format_flags": Counter(),
        "input_bytes_unchanged": True,
    }
    for path in sorted(root.rglob("Form.xml")):
        before = path.read_bytes()
        document = ET.fromstring(before)
        if document.tag != FORM + "Form":
            raise ValueError("Unaccepted form root")
        parents = {child: parent for parent in document.iter() for child in parent}
        result["forms"] += 1
        result["root_profiles"][document.attrib.get("version", "absent")] += 1
        result["root_sections"].update(local(child.tag) for child in document)
        scopes = {key: [] for key in ("elements", "attributes", "commands")}
        names = {key: {} for key in scopes}
        regular_names = {}
        for node in document.iter():
            kind = local(node.tag)
            if not node.tag.startswith(FORM) or "id" not in node.attrib:
                continue
            identity = node.attrib["id"]
            int(identity)
            result["id_kinds"][kind] += 1
            if identity.startswith("-"):
                result["negative_ids"][kind + ":" + identity] += 1
            name = node.attrib.get("name", "").lower()
            if not name:
                result["empty_names_by_kind"][kind] += 1
            if kind == "Column":
                continue
            scope = "attributes" if kind == "Attribute" else "commands" if kind == "Command" else "elements"
            scopes[scope].append(identity)
            result["namespace_name_collisions"][scope] += int(name in names[scope])
            names[scope][name] = node
            if kind in DATA_KINDS:
                continue
            parent = parents[node]
            if parent.tag == FORM + "ChildItems":
                result["regular_element_kinds"][kind] += 1
                grandparent = parents[parent]
                result["regular_parent_kind_pairs"][local(grandparent.tag) + "/" + kind] += 1
                regular_names[name] = node
            else:
                result["fixed_element_kinds"][kind] += 1
                result["fixed_parent_kind_pairs"][local(parent.tag) + "/" + kind] += 1
        for scope, identities in scopes.items():
            result["namespace_id_collisions"][scope] += len(identities) - len(set(identities))
        for left, right in [("elements", "attributes"), ("elements", "commands"), ("attributes", "commands")]:
            result["forms_with_id_scope_overlap"][left + "/" + right] += int(bool(set(scopes[left]) & set(scopes[right])))
        for parent in document.iter():
            columns = [child for child in parent if child.tag == FORM + "Column"]
            if not columns:
                continue
            result["column_containers"] += 1
            identities = [node.attrib["id"] for node in columns]
            column_names = [node.attrib["name"].lower() for node in columns]
            result["column_container_id_collisions"] += len(identities) - len(set(identities))
            result["column_container_name_collisions"] += len(column_names) - len(set(column_names))
        column_ids = [node.attrib["id"] for node in document.iter(FORM + "Column")]
        result["forms_with_repeated_column_ids_across_containers"] += int(len(column_ids) != len(set(column_ids)))
        for attribute in document.findall(FORM + "Attributes/" + FORM + "Attribute"):
            column_ids = [node.attrib["id"] for node in attribute.iter(FORM + "Column")]
            result["attributes_with_repeated_column_ids_across_containers"] += int(len(column_ids) != len(set(column_ids)))
        for source in document.iter(FORM + "AdditionSource"):
            item = source.find(FORM + "Item")
            if item is None:
                continue
            target = regular_names.get((item.text or "").lower())
            result["references"]["addition_source"] += 1
            result["unresolved_known_references"]["addition_source"] += int(target is None or target.tag != FORM + "Table")
        for command in document.iter(FORM + "CommandName"):
            value = command.text or ""
            if value.startswith("Form.Command."):
                shape = "local_command"
                result["unresolved_known_references"][shape] += int(value[len("Form.Command."):].lower() not in names["commands"])
            elif value.startswith("Form.StandardCommand."):
                shape = "standard_command"
            elif value.lstrip("-").isdigit():
                shape = "numeric_command"
            else:
                shape = "other_command"
            result["references"][shape] += 1
        for data in document.iter(FORM + "DataPath"):
            result["data_path_shapes"][data_path_shape(data.text or "", names["attributes"])] += 1
        for title in document.iter(FORM + "Title"):
            if "formatted" in title.attrib:
                result["title_format_flags"][title.attrib["formatted"]] += 1
        if hashlib.sha256(before).digest() != hashlib.sha256(path.read_bytes()).digest():
            raise ValueError("Input changed during inventory")
    result["regular_elements"] = sum(result["regular_element_kinds"].values())
    result["fixed_elements"] = sum(result["fixed_element_kinds"].values())
    result["total_ui_elements"] = result["regular_elements"] + result["fixed_elements"]
    result["native_namespace"] = FORM_NS
    return result


def main():
    """Print anonymized evidence; research output never replaces a native input file."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("native_root", type=Path)
    arguments = parser.parse_args()
    root = arguments.native_root.resolve(strict=True)
    if not root.is_dir():
        parser.error("native_root must be a directory")
    print(json.dumps(collect(root), ensure_ascii=False, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
