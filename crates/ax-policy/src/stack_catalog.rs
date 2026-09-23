// Generated catalog of embedded stack packs. Edit templates, then keep this list in sync.
pub struct StackFile { pub rel: &'static str, pub body: &'static str }
pub struct StackDef {
    pub id: &'static str,
    pub version: &'static str,
    pub description: &'static str,
    pub depends_on: &'static [&'static str],
    pub files: &'static [StackFile],
}
const DOTNET_FILES: &[StackFile] = &[
    StackFile { rel: "rules/dotnet-aspnet.mdc", body: include_str!("../templates/stacks/dotnet/rules/dotnet-aspnet.mdc") },
    StackFile { rel: "rules/dotnet-async.mdc", body: include_str!("../templates/stacks/dotnet/rules/dotnet-async.mdc") },
    StackFile { rel: "rules/dotnet-ef.mdc", body: include_str!("../templates/stacks/dotnet/rules/dotnet-ef.mdc") },
    StackFile { rel: "rules/dotnet-nullable.mdc", body: include_str!("../templates/stacks/dotnet/rules/dotnet-nullable.mdc") },
    StackFile { rel: "skills/dotnet-code-review/SKILL.md", body: include_str!("../templates/stacks/dotnet/skills/dotnet-code-review/SKILL.md") },
];
const JAVA_FILES: &[StackFile] = &[
    StackFile { rel: "rules/java-null-safety.mdc", body: include_str!("../templates/stacks/java/rules/java-null-safety.mdc") },
    StackFile { rel: "rules/java-tests.mdc", body: include_str!("../templates/stacks/java/rules/java-tests.mdc") },
    StackFile { rel: "skills/java-review/SKILL.md", body: include_str!("../templates/stacks/java/skills/java-review/SKILL.md") },
];
const REACT_FILES: &[StackFile] = &[
    StackFile { rel: "rules/react-hooks.mdc", body: include_str!("../templates/stacks/react/rules/react-hooks.mdc") },
    StackFile { rel: "rules/react-state.mdc", body: include_str!("../templates/stacks/react/rules/react-state.mdc") },
    StackFile { rel: "skills/react-review/SKILL.md", body: include_str!("../templates/stacks/react/skills/react-review/SKILL.md") },
];
const NEXTJS_FILES: &[StackFile] = &[
    StackFile { rel: "rules/nextjs-app-router.mdc", body: include_str!("../templates/stacks/nextjs/rules/nextjs-app-router.mdc") },
    StackFile { rel: "rules/nextjs-server-components.mdc", body: include_str!("../templates/stacks/nextjs/rules/nextjs-server-components.mdc") },
    StackFile { rel: "skills/nextjs-review/SKILL.md", body: include_str!("../templates/stacks/nextjs/skills/nextjs-review/SKILL.md") },
];
const ANGULAR_FILES: &[StackFile] = &[
    StackFile { rel: "rules/angular-di.mdc", body: include_str!("../templates/stacks/angular/rules/angular-di.mdc") },
    StackFile { rel: "rules/angular-templates.mdc", body: include_str!("../templates/stacks/angular/rules/angular-templates.mdc") },
    StackFile { rel: "skills/angular-review/SKILL.md", body: include_str!("../templates/stacks/angular/skills/angular-review/SKILL.md") },
];
const VUE_FILES: &[StackFile] = &[
    StackFile { rel: "rules/vue-composition.mdc", body: include_str!("../templates/stacks/vue/rules/vue-composition.mdc") },
    StackFile { rel: "rules/vue-sfc.mdc", body: include_str!("../templates/stacks/vue/rules/vue-sfc.mdc") },
    StackFile { rel: "skills/vue-review/SKILL.md", body: include_str!("../templates/stacks/vue/skills/vue-review/SKILL.md") },
];
const PHP_FILES: &[StackFile] = &[
    StackFile { rel: "rules/php-security.mdc", body: include_str!("../templates/stacks/php/rules/php-security.mdc") },
    StackFile { rel: "rules/php-types.mdc", body: include_str!("../templates/stacks/php/rules/php-types.mdc") },
    StackFile { rel: "skills/php-review/SKILL.md", body: include_str!("../templates/stacks/php/skills/php-review/SKILL.md") },
];
const LARAVEL_FILES: &[StackFile] = &[
    StackFile { rel: "rules/laravel-eloquent.mdc", body: include_str!("../templates/stacks/laravel/rules/laravel-eloquent.mdc") },
    StackFile { rel: "rules/laravel-validation.mdc", body: include_str!("../templates/stacks/laravel/rules/laravel-validation.mdc") },
    StackFile { rel: "skills/laravel-review/SKILL.md", body: include_str!("../templates/stacks/laravel/skills/laravel-review/SKILL.md") },
];
const DRUPAL_FILES: &[StackFile] = &[
    StackFile { rel: "rules/drupal-config.mdc", body: include_str!("../templates/stacks/drupal/rules/drupal-config.mdc") },
    StackFile { rel: "rules/drupal-hooks.mdc", body: include_str!("../templates/stacks/drupal/rules/drupal-hooks.mdc") },
    StackFile { rel: "skills/drupal-review/SKILL.md", body: include_str!("../templates/stacks/drupal/skills/drupal-review/SKILL.md") },
];
const SITECORE_FILES: &[StackFile] = &[
    StackFile { rel: "rules/sitecore-helix.mdc", body: include_str!("../templates/stacks/sitecore/rules/sitecore-helix.mdc") },
    StackFile { rel: "rules/sitecore-serialization.mdc", body: include_str!("../templates/stacks/sitecore/rules/sitecore-serialization.mdc") },
    StackFile { rel: "skills/sitecore-review/SKILL.md", body: include_str!("../templates/stacks/sitecore/skills/sitecore-review/SKILL.md") },
];
const OPTIMIZELY_FILES: &[StackFile] = &[
    StackFile { rel: "rules/optimizely-content.mdc", body: include_str!("../templates/stacks/optimizely/rules/optimizely-content.mdc") },
    StackFile { rel: "rules/optimizely-initialization.mdc", body: include_str!("../templates/stacks/optimizely/rules/optimizely-initialization.mdc") },
    StackFile { rel: "skills/optimizely-review/SKILL.md", body: include_str!("../templates/stacks/optimizely/skills/optimizely-review/SKILL.md") },
];
const ASTRO_FILES: &[StackFile] = &[
    StackFile { rel: "rules/astro-content.mdc", body: include_str!("../templates/stacks/astro/rules/astro-content.mdc") },
    StackFile { rel: "rules/astro-islands.mdc", body: include_str!("../templates/stacks/astro/rules/astro-islands.mdc") },
    StackFile { rel: "skills/astro-review/SKILL.md", body: include_str!("../templates/stacks/astro/skills/astro-review/SKILL.md") },
];
const C_FILES: &[StackFile] = &[
    StackFile { rel: "rules/c-errors.mdc", body: include_str!("../templates/stacks/c/rules/c-errors.mdc") },
    StackFile { rel: "rules/c-memory.mdc", body: include_str!("../templates/stacks/c/rules/c-memory.mdc") },
    StackFile { rel: "skills/c-review/SKILL.md", body: include_str!("../templates/stacks/c/skills/c-review/SKILL.md") },
];
const CPP_FILES: &[StackFile] = &[
    StackFile { rel: "rules/cpp-errors.mdc", body: include_str!("../templates/stacks/cpp/rules/cpp-errors.mdc") },
    StackFile { rel: "rules/cpp-raii.mdc", body: include_str!("../templates/stacks/cpp/rules/cpp-raii.mdc") },
    StackFile { rel: "skills/cpp-review/SKILL.md", body: include_str!("../templates/stacks/cpp/skills/cpp-review/SKILL.md") },
];
const DART_FILES: &[StackFile] = &[
    StackFile { rel: "rules/dart-null.mdc", body: include_str!("../templates/stacks/dart/rules/dart-null.mdc") },
    StackFile { rel: "rules/dart-packages.mdc", body: include_str!("../templates/stacks/dart/rules/dart-packages.mdc") },
    StackFile { rel: "skills/dart-review/SKILL.md", body: include_str!("../templates/stacks/dart/skills/dart-review/SKILL.md") },
];
const GO_FILES: &[StackFile] = &[
    StackFile { rel: "rules/go-context.mdc", body: include_str!("../templates/stacks/go/rules/go-context.mdc") },
    StackFile { rel: "rules/go-errors.mdc", body: include_str!("../templates/stacks/go/rules/go-errors.mdc") },
    StackFile { rel: "rules/go-tests.mdc", body: include_str!("../templates/stacks/go/rules/go-tests.mdc") },
    StackFile { rel: "skills/go-review/SKILL.md", body: include_str!("../templates/stacks/go/skills/go-review/SKILL.md") },
];
const JAVASCRIPT_FILES: &[StackFile] = &[
    StackFile { rel: "rules/javascript-async.mdc", body: include_str!("../templates/stacks/javascript/rules/javascript-async.mdc") },
    StackFile { rel: "rules/javascript-checks.mdc", body: include_str!("../templates/stacks/javascript/rules/javascript-checks.mdc") },
    StackFile { rel: "rules/javascript-modules.mdc", body: include_str!("../templates/stacks/javascript/rules/javascript-modules.mdc") },
    StackFile { rel: "skills/javascript-review/SKILL.md", body: include_str!("../templates/stacks/javascript/skills/javascript-review/SKILL.md") },
];
const KOTLIN_FILES: &[StackFile] = &[
    StackFile { rel: "rules/kotlin-coroutines.mdc", body: include_str!("../templates/stacks/kotlin/rules/kotlin-coroutines.mdc") },
    StackFile { rel: "rules/kotlin-null.mdc", body: include_str!("../templates/stacks/kotlin/rules/kotlin-null.mdc") },
    StackFile { rel: "skills/kotlin-review/SKILL.md", body: include_str!("../templates/stacks/kotlin/skills/kotlin-review/SKILL.md") },
];
const LUA_FILES: &[StackFile] = &[
    StackFile { rel: "rules/lua-errors.mdc", body: include_str!("../templates/stacks/lua/rules/lua-errors.mdc") },
    StackFile { rel: "rules/lua-modules.mdc", body: include_str!("../templates/stacks/lua/rules/lua-modules.mdc") },
    StackFile { rel: "skills/lua-review/SKILL.md", body: include_str!("../templates/stacks/lua/skills/lua-review/SKILL.md") },
];
const LUAU_FILES: &[StackFile] = &[
    StackFile { rel: "rules/luau-modules.mdc", body: include_str!("../templates/stacks/luau/rules/luau-modules.mdc") },
    StackFile { rel: "rules/luau-types.mdc", body: include_str!("../templates/stacks/luau/rules/luau-types.mdc") },
    StackFile { rel: "skills/luau-review/SKILL.md", body: include_str!("../templates/stacks/luau/skills/luau-review/SKILL.md") },
];
const OBJC_FILES: &[StackFile] = &[
    StackFile { rel: "rules/objc-memory.mdc", body: include_str!("../templates/stacks/objc/rules/objc-memory.mdc") },
    StackFile { rel: "rules/objc-nullability.mdc", body: include_str!("../templates/stacks/objc/rules/objc-nullability.mdc") },
    StackFile { rel: "skills/objc-review/SKILL.md", body: include_str!("../templates/stacks/objc/skills/objc-review/SKILL.md") },
];
const PASCAL_FILES: &[StackFile] = &[
    StackFile { rel: "rules/pascal-memory.mdc", body: include_str!("../templates/stacks/pascal/rules/pascal-memory.mdc") },
    StackFile { rel: "rules/pascal-units.mdc", body: include_str!("../templates/stacks/pascal/rules/pascal-units.mdc") },
    StackFile { rel: "skills/pascal-review/SKILL.md", body: include_str!("../templates/stacks/pascal/skills/pascal-review/SKILL.md") },
];
const PYTHON_FILES: &[StackFile] = &[
    StackFile { rel: "rules/python-async.mdc", body: include_str!("../templates/stacks/python/rules/python-async.mdc") },
    StackFile { rel: "rules/python-tests.mdc", body: include_str!("../templates/stacks/python/rules/python-tests.mdc") },
    StackFile { rel: "rules/python-types.mdc", body: include_str!("../templates/stacks/python/rules/python-types.mdc") },
    StackFile { rel: "skills/python-review/SKILL.md", body: include_str!("../templates/stacks/python/skills/python-review/SKILL.md") },
];
const R_FILES: &[StackFile] = &[
    StackFile { rel: "rules/r-functions.mdc", body: include_str!("../templates/stacks/r/rules/r-functions.mdc") },
    StackFile { rel: "rules/r-packages.mdc", body: include_str!("../templates/stacks/r/rules/r-packages.mdc") },
    StackFile { rel: "skills/r-review/SKILL.md", body: include_str!("../templates/stacks/r/skills/r-review/SKILL.md") },
];
const RUBY_FILES: &[StackFile] = &[
    StackFile { rel: "rules/ruby-style.mdc", body: include_str!("../templates/stacks/ruby/rules/ruby-style.mdc") },
    StackFile { rel: "rules/ruby-tests.mdc", body: include_str!("../templates/stacks/ruby/rules/ruby-tests.mdc") },
    StackFile { rel: "skills/ruby-review/SKILL.md", body: include_str!("../templates/stacks/ruby/skills/ruby-review/SKILL.md") },
];
const RUST_FILES: &[StackFile] = &[
    StackFile { rel: "rules/rust-async.mdc", body: include_str!("../templates/stacks/rust/rules/rust-async.mdc") },
    StackFile { rel: "rules/rust-errors.mdc", body: include_str!("../templates/stacks/rust/rules/rust-errors.mdc") },
    StackFile { rel: "rules/rust-tests.mdc", body: include_str!("../templates/stacks/rust/rules/rust-tests.mdc") },
    StackFile { rel: "skills/rust-review/SKILL.md", body: include_str!("../templates/stacks/rust/skills/rust-review/SKILL.md") },
];
const SCALA_FILES: &[StackFile] = &[
    StackFile { rel: "rules/scala-immutability.mdc", body: include_str!("../templates/stacks/scala/rules/scala-immutability.mdc") },
    StackFile { rel: "rules/scala-tests.mdc", body: include_str!("../templates/stacks/scala/rules/scala-tests.mdc") },
    StackFile { rel: "skills/scala-review/SKILL.md", body: include_str!("../templates/stacks/scala/skills/scala-review/SKILL.md") },
];
const SVELTE_FILES: &[StackFile] = &[
    StackFile { rel: "rules/svelte-components.mdc", body: include_str!("../templates/stacks/svelte/rules/svelte-components.mdc") },
    StackFile { rel: "rules/svelte-state.mdc", body: include_str!("../templates/stacks/svelte/rules/svelte-state.mdc") },
    StackFile { rel: "skills/svelte-review/SKILL.md", body: include_str!("../templates/stacks/svelte/skills/svelte-review/SKILL.md") },
];
const SWIFT_FILES: &[StackFile] = &[
    StackFile { rel: "rules/swift-optionals.mdc", body: include_str!("../templates/stacks/swift/rules/swift-optionals.mdc") },
    StackFile { rel: "rules/swift-types.mdc", body: include_str!("../templates/stacks/swift/rules/swift-types.mdc") },
    StackFile { rel: "skills/swift-review/SKILL.md", body: include_str!("../templates/stacks/swift/skills/swift-review/SKILL.md") },
];
const TYPESCRIPT_FILES: &[StackFile] = &[
    StackFile { rel: "rules/typescript-async.mdc", body: include_str!("../templates/stacks/typescript/rules/typescript-async.mdc") },
    StackFile { rel: "rules/typescript-modules.mdc", body: include_str!("../templates/stacks/typescript/rules/typescript-modules.mdc") },
    StackFile { rel: "rules/typescript-types.mdc", body: include_str!("../templates/stacks/typescript/rules/typescript-types.mdc") },
    StackFile { rel: "skills/typescript-review/SKILL.md", body: include_str!("../templates/stacks/typescript/skills/typescript-review/SKILL.md") },
];
pub static STACKS: &[StackDef] = &[
    StackDef { id: "dotnet", version: "1.2.0", description: ".NET and C# review rules", depends_on: &[], files: DOTNET_FILES },
    StackDef { id: "java", version: "1.2.0", description: "Java, Maven, and Gradle review rules", depends_on: &[], files: JAVA_FILES },
    StackDef { id: "react", version: "1.2.0", description: "React component and state rules", depends_on: &[], files: REACT_FILES },
    StackDef { id: "nextjs", version: "1.2.0", description: "Next.js App Router rules", depends_on: &["react"], files: NEXTJS_FILES },
    StackDef { id: "angular", version: "1.2.0", description: "Angular dependency injection and template rules", depends_on: &[], files: ANGULAR_FILES },
    StackDef { id: "vue", version: "1.2.0", description: "Vue composition API and single-file component rules", depends_on: &[], files: VUE_FILES },
    StackDef { id: "php", version: "1.2.0", description: "PHP type and security rules", depends_on: &[], files: PHP_FILES },
    StackDef { id: "laravel", version: "1.2.0", description: "Laravel Eloquent and validation rules", depends_on: &["php"], files: LARAVEL_FILES },
    StackDef { id: "drupal", version: "1.2.0", description: "Drupal hook and configuration rules", depends_on: &["php"], files: DRUPAL_FILES },
    StackDef { id: "sitecore", version: "1.2.0", description: "Sitecore Helix and serialization rules", depends_on: &["dotnet"], files: SITECORE_FILES },
    StackDef { id: "optimizely", version: "1.2.0", description: "Optimizely content and initialization rules", depends_on: &["dotnet"], files: OPTIMIZELY_FILES },
    StackDef { id: "astro", version: "1.2.0", description: "Astro islands and content", depends_on: &[], files: ASTRO_FILES },
    StackDef { id: "c", version: "1.2.0", description: "C memory, headers, and errors", depends_on: &[], files: C_FILES },
    StackDef { id: "cpp", version: "1.2.0", description: "C++ ownership, RAII, and errors", depends_on: &[], files: CPP_FILES },
    StackDef { id: "dart", version: "1.2.0", description: "Dart null-safety and widgets or libraries", depends_on: &[], files: DART_FILES },
    StackDef { id: "go", version: "1.2.0", description: "Go errors, packages, and tests", depends_on: &[], files: GO_FILES },
    StackDef { id: "javascript", version: "1.2.0", description: "JavaScript modules and runtime checks", depends_on: &[], files: JAVASCRIPT_FILES },
    StackDef { id: "kotlin", version: "1.2.0", description: "Kotlin null-safety, coroutines, and tests", depends_on: &[], files: KOTLIN_FILES },
    StackDef { id: "lua", version: "1.2.0", description: "Lua modules and errors", depends_on: &[], files: LUA_FILES },
    StackDef { id: "luau", version: "1.2.0", description: "Luau types and modules", depends_on: &[], files: LUAU_FILES },
    StackDef { id: "objc", version: "1.2.0", description: "Objective-C memory and nullability", depends_on: &[], files: OBJC_FILES },
    StackDef { id: "pascal", version: "1.2.0", description: "Pascal units, types, and memory", depends_on: &[], files: PASCAL_FILES },
    StackDef { id: "python", version: "1.2.0", description: "Python types, packaging, and tests", depends_on: &[], files: PYTHON_FILES },
    StackDef { id: "r", version: "1.2.0", description: "R functions, vectors, and packages", depends_on: &[], files: R_FILES },
    StackDef { id: "ruby", version: "1.2.0", description: "Ruby style, gems, and tests", depends_on: &[], files: RUBY_FILES },
    StackDef { id: "rust", version: "1.2.0", description: "Rust ownership, errors, and tests", depends_on: &[], files: RUST_FILES },
    StackDef { id: "scala", version: "1.2.0", description: "Scala types, immutability, and tests", depends_on: &[], files: SCALA_FILES },
    StackDef { id: "svelte", version: "1.2.0", description: "Svelte components and stores", depends_on: &[], files: SVELTE_FILES },
    StackDef { id: "swift", version: "1.2.0", description: "Swift optionals, value types, and tests", depends_on: &[], files: SWIFT_FILES },
    StackDef { id: "typescript", version: "1.2.0", description: "TypeScript types and modules", depends_on: &[], files: TYPESCRIPT_FILES },
];
