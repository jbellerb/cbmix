load("@prelude//:paths.bzl", "paths")

def debian_package_impl(ctx: "context") -> ["provider"]:
    source = ctx.actions.copied_dir(
        "__{}_debian_package__".format(ctx.label.name),
        {p.short_path: p for p in ctx.attrs.root} | ctx.attrs.extras | {
            "DEBIAN/control": ctx.attrs.control,
        },
    )
    output = ctx.actions.declare_output(ctx.label.name + ".deb")

    cmd = cmd_args(["dpkg-deb", "-b", source, output.as_output()])
    ctx.actions.run(cmd, category = "build_deb")

    return [
        DefaultInfo(default_output = output),
    ]

debian_package = rule(impl = debian_package_impl, attrs = {
  "root": attrs.list(attrs.source(), default = [], doc = """
    The set of files to include in the package. Must follow FSH conventions.
"""),
  "extras": attrs.dict(attrs.string(), attrs.source(), default = {}, doc = """
    Extra files to be placed at specific locations in the FSH.
"""),
  "control": attrs.source(doc = """
    The control metadata file for the package.
"""),
})
