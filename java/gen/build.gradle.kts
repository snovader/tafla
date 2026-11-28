import com.dylibso.chicory.build.time.compiler.Config
import com.dylibso.chicory.build.time.compiler.Generator
import com.dylibso.chicory.compiler.InterpreterFallback
import com.vanniktech.maven.publish.DeploymentValidation
import com.vanniktech.maven.publish.JavaLibrary
import com.vanniktech.maven.publish.JavadocJar
import com.vanniktech.maven.publish.SourcesJar
import net.ltgt.gradle.errorprone.errorprone

// Chicory's build-time compiler, which CompileGeneratorModule below runs. It's on the build's
// classpath alone: the jar holds the classes that it generates, and Chicory's runtime.
buildscript {
    repositories {
        mavenCentral()
    }
    dependencies {
        classpath(libs.chicory.build.time.compiler)
    }
}

plugins {
    java
    signing
    alias(libs.plugins.errorprone)
    alias(libs.plugins.maven.publish)
}

group = "io.github.snovader.tafla"
// tafla-gen's version, which scripts/version.sh writes here from rust/gen/Cargo.toml's
version = "0.0.0"

// The project is gen, after its directory, and the artifact is tafla-gen
base {
    archivesName = "tafla-gen"
}

// Chicory's runtime, whose classes the jar holds, so that `java -jar` runs it and its POM
// depends on nothing
val bundled = configurations.dependencyScope("bundled")
val bundledClasspath = configurations.resolvable("bundledClasspath") {
    extendsFrom(bundled.get())
}

dependencies {
    "bundled"(libs.chicory.runtime)
    compileOnly(libs.chicory.runtime)

    testImplementation(libs.junit.jupiter)

    testRuntimeOnly(libs.junit.platform.launcher)

    errorprone(libs.errorprone.core)
    errorprone(libs.nullaway)
}

java {
    toolchain {
        languageVersion = JavaLanguageVersion.of(25)
    }
}

val repository = rootProject.layout.projectDirectory.dir("..")
val rust = repository.dir("rust")

// Builds tafla-gen's WebAssembly module, as rust/gen/README.md describes, which needs Rust with
// the wasm32-unknown-unknown target. cargo decides what to rebuild, in the Rust workspace's
// target directory.
val buildGeneratorModule = tasks.register<Exec>("buildGeneratorModule") {
    workingDir = rust.asFile
    commandLine(
        "cargo", "build", "-p", "tafla-gen", "--lib",
        "--target", "wasm32-unknown-unknown", "--profile", "wasm"
    )

    inputs.files(rust.file("Cargo.toml"), rust.file("Cargo.lock"), rust.file("rust-toolchain.toml"))
    inputs.file(rust.file("gen/Cargo.toml"))
    inputs.dir(rust.dir("gen/src"))
    outputs.file(rust.file("target/wasm32-unknown-unknown/wasm/tafla_gen.wasm"))
}

/**
 * Compiles a WebAssembly module to JVM classes with Chicory's build-time compiler: the classes
 * of the module's functions, the source of the class that loads them, named `className`, and
 * the module without its code, which that class reads as a resource. Run so, the generator
 * takes a third of the time that Chicory's interpreter takes to start and compile a schema.
 */
abstract class CompileGeneratorModule : DefaultTask() {
    @get:InputFile
    @get:PathSensitive(PathSensitivity.NONE)
    abstract val module: RegularFileProperty

    @get:Input
    abstract val className: Property<String>

    @get:OutputDirectory
    abstract val sources: DirectoryProperty

    @get:OutputDirectory
    abstract val classes: DirectoryProperty

    @get:OutputDirectory
    abstract val resources: DirectoryProperty

    @get:Inject
    abstract val files: FileSystemOperations

    @TaskAction
    fun compile() {
        files.delete { delete(sources, classes, resources) }

        val config = Config.builder()
            .withWasmFile(module.get().asFile.toPath())
            .withName(className.get())
            .withTargetSourceFolder(sources.get().asFile.toPath())
            .withTargetClassFolder(classes.get().asFile.toPath())
            .withTargetWasmFolder(resources.get().asFile.toPath())
            // Fails the build rather than interpreting a function that's too large to compile
            .withInterpreterFallback(InterpreterFallback.FAIL)
            .build()
        val generator = Generator(config)

        val interpretedFunctions = generator.generateResources()
        generator.generateSources()
        generator.generateMetaWasm(interpretedFunctions)
    }
}

val compileGeneratorModule = tasks.register<CompileGeneratorModule>("compileGeneratorModule") {
    module = layout.file(buildGeneratorModule.map { it.outputs.files.singleFile })
    className = "io.github.snovader.tafla.gen.CompiledGenerator"

    val generated = layout.buildDirectory.dir("generated/chicory")
    sources = generated.map { it.dir("java") }
    classes = generated.map { it.dir("classes") }
    resources = generated.map { it.dir("resources") }
}

val compiledClasses = files(compileGeneratorModule.flatMap { it.classes })

sourceSets.main {
    java.srcDir(compileGeneratorModule.flatMap { it.sources })
    resources.srcDir(compileGeneratorModule.flatMap { it.resources })
    // The class that loads the module's classes refers to them
    compileClasspath += compiledClasses
    output.dir(compiledClasses)
}

tasks.withType<JavaCompile>().configureEach {
    options.release = 21
    // Error Prone reports most checks as warnings and has no flag of its own to fail on them.
    options.compilerArgs.add("-Werror")
    options.errorprone {
        // The class that Chicory generates
        excludedPaths = ".*/build/generated/.*"
        error("NullAway")
        option("NullAway:OnlyNullMarked", true)
        option("NullAway:JSpecifyMode", true)
    }
}

tasks.named<Jar>("jar") {
    manifest {
        attributes("Main-Class" to "io.github.snovader.tafla.gen.Main")
    }

    // Chicory's classes, without the descriptors of its modules, since the jar is none of them
    from(bundledClasspath.map { classpath -> classpath.map { zipTree(it) } }) {
        exclude("META-INF/**", "module-info.class")
    }
    // tafla's license, at the root of the repository, and Chicory's
    from(repository.file("LICENSE")) {
        into("META-INF")
    }
    from(layout.projectDirectory.file("licenses/LICENSE-chicory")) {
        into("META-INF")
    }
}

// The sources and the javadoc hold none of Chicory's
tasks.withType<Jar>().matching { it.name != "jar" }.configureEach {
    from(repository.file("LICENSE")) {
        into("META-INF")
    }
}

tasks.named<Javadoc>("javadoc") {
    // The command's class alone: the rest is the jar's own, and Chicory's generated class
    setSource(sourceSets.main.get().java.matching { include("**/gen/Main.java", "**/package-info.java") })
}

// Runs the jar's command, as its users do, with the cases of rust/gen/tests/cli.rs
tasks.named<Test>("test") {
    javaLauncher = javaToolchains.launcherFor {
        languageVersion = providers.gradleProperty("testJavaVersion")
            .map { JavaLanguageVersion.of(it) }
            .orElse(JavaLanguageVersion.of(25))
    }

    useJUnitPlatform()

    val jar = tasks.named<Jar>("jar").flatMap { it.archiveFile }
    inputs.file(jar)
    // The example schemas, the invalid schemas and their errors, and the golden files
    inputs.dir(repository.dir("schemas"))
    inputs.dir(rust.dir("gen/tests/diagnostics"))
    inputs.dir(rust.dir("gen/tests/golden"))
    inputs.dir(repository.dir("java/core/src/test/java/io/github/snovader/tafla/test/generated"))

    val repositoryPath = repository.asFile.absolutePath
    val commandVersion = version.toString()
    doFirst {
        systemProperty("tafla.gen.jar", jar.get().asFile.absolutePath)
        systemProperty("tafla.repository", repositoryPath)
        systemProperty("tafla.gen.version", commandVersion)
    }
}

// Publishes the jar, its sources, its javadoc and its POM to Maven Central through the Central
// Portal, as the runtime's project does and .github/workflows/release.yml runs
mavenPublishing {
    configure(JavaLibrary(javadocJar = JavadocJar.Javadoc(), sourcesJar = SourcesJar.Sources()))
    coordinates(group.toString(), base.archivesName.get(), version.toString())

    publishToMavenCentral(
        automaticRelease = true,
        validateDeployment = DeploymentValidation.VALIDATED
    )

    signAllPublications()

    pom {
        name = "tafla-gen"
        description = "Compiles tafla schemas into code that writes and reads tables in " +
            "TypeScript, Python, Java, Rust, Go, C++, Swift and C#"
        url = "https://github.com/snovader/tafla"
        licenses {
            license {
                name = "MIT"
                url = "https://opensource.org/license/mit"
            }
            // Chicory's runtime, whose classes the jar holds
            license {
                name = "Apache-2.0"
                url = "https://www.apache.org/licenses/LICENSE-2.0.txt"
            }
        }
        developers {
            developer {
                id = "snovader"
                name = "snovader"
                url = "https://github.com/snovader"
            }
        }
        scm {
            connection = "scm:git:https://github.com/snovader/tafla.git"
            developerConnection = "scm:git:ssh://git@github.com/snovader/tafla.git"
            url = "https://github.com/snovader/tafla"
        }
        issueManagement {
            system = "GitHub"
            url = "https://github.com/snovader/tafla/issues"
        }
    }
}

// Central rejects unsigned artifacts, so publishing fails without a key. Building signs nothing.
signing {
    isRequired = true
}
