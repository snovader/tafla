import com.vanniktech.maven.publish.DeploymentValidation
import com.vanniktech.maven.publish.JavaLibrary
import com.vanniktech.maven.publish.JavadocJar
import com.vanniktech.maven.publish.SourcesJar
import net.ltgt.gradle.errorprone.errorprone

plugins {
    `java-library`
    signing
    alias(libs.plugins.errorprone)
    alias(libs.plugins.maven.publish)
}

group = "io.github.snovader.tafla"
version = "0.0.0"

// The project is core, after its directory, and the artifact is tafla, as README.md names it
base {
    archivesName = "tafla"
}

dependencies {
    api(libs.jspecify)

    testImplementation(libs.jakarta.json.api)
    testImplementation(libs.jqwik)
    testImplementation(libs.junit.jupiter)

    testRuntimeOnly(libs.junit.platform.launcher)
    testRuntimeOnly(libs.parsson)

    errorprone(libs.errorprone.core)
    errorprone(libs.nullaway)
}

java {
    toolchain {
        languageVersion = JavaLanguageVersion.of(25)
    }
}

// The license at the root of the repository, in every jar
tasks.withType<Jar>().configureEach {
    from(rootProject.layout.projectDirectory.file("../LICENSE")) {
        into("META-INF")
    }
}

tasks.withType<JavaCompile>().configureEach {
    options.release = 21
    // Error Prone reports most checks as warnings and has no flag of its own to fail on them.
    options.compilerArgs.add("-Werror")
    options.errorprone {
        error("NullAway")
        option("NullAway:OnlyNullMarked", true)
        option("NullAway:JSpecifyMode", true)
    }
}

tasks.named<JavaCompile>("compileTestJava") {
    options.errorprone {
        // The schema of absent fields and versions that the tests compile the generated Java of,
        // typescript/core/test/presence.tafla, declares a Record, as java.lang does
        disable("AvoidCommonTypeNames")
    }
}

tasks.named<Test>("test") {
    javaLauncher = javaToolchains.launcherFor {
        languageVersion = providers.gradleProperty("testJavaVersion")
            .map { JavaLanguageVersion.of(it) }
            .orElse(JavaLanguageVersion.of(25))
    }

    useJUnitPlatform {
        includeEngines("jqwik")
    }

    // The conformance suite at the root of the repository, with the cases of the generated code in
    // its generated/, see conformance/README.md
    val conformance = rootProject.layout.projectDirectory.dir("../conformance")
    inputs.dir(conformance)
    systemProperty("tafla.conformance", conformance.asFile.absolutePath)
}

// Publishes the jar, its sources, its javadoc and its POM to Maven Central through the Central
// Portal, as .github/workflows/release.yml does, with the mavenCentralUsername and
// mavenCentralPassword properties, such as ORG_GRADLE_PROJECT_mavenCentralUsername in the
// environment. publishToMavenLocal publishes them to the local Maven repository.
mavenPublishing {
    configure(JavaLibrary(javadocJar = JavadocJar.Javadoc(), sourcesJar = SourcesJar.Sources()))
    coordinates(group.toString(), base.archivesName.get(), version.toString())

    // Publishes the deployment once the Central Portal has validated it, rather than leaving it to
    // be published by hand
    publishToMavenCentral(
        automaticRelease = true,
        validateDeployment = DeploymentValidation.VALIDATED
    )

    // Signs every publication with an ASCII-armored key in the signingInMemoryKey and
    // signingInMemoryKeyPassword properties
    signAllPublications()

    pom {
        name = "tafla"
        description = "Binary tables of scalars, strings, arrays and structs, read with bounds checks"
        url = "https://github.com/snovader/tafla"
        licenses {
            license {
                name = "MIT"
                url = "https://opensource.org/license/mit"
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

// Central rejects unsigned artifacts, so publishing, to Maven Central or the local repository,
// fails without a key rather than publishing them unsigned. Building signs nothing.
signing {
    isRequired = true
}
