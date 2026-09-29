import com._1c.g5.v8.dt.metadata.mdclass.*;
import com._1c.g5.v8.dt.md.export.xml.impl.MetadataFeatureNameProvider;
import com._1c.g5.v8.dt.md.export.xml.writer.MetadataObjectWriter;
import com._1c.g5.v8.dt.internal.md.export.xml.writer.MetadataObjectFeatureOrderProvider;
import com._1c.g5.v8.dt.export.xml.IExportContext;
import com._1c.g5.v8.dt.platform.version.Version;
import org.eclipse.emf.ecore.*;
import org.eclipse.core.resources.IProject;
import com.google.gson.GsonBuilder;
import java.util.*;

/** Offline inspection of installed EDT; never loaded by eska or its build. */
class Inspect extends MetadataObjectWriter {
    /** Only version rules are needed; no Eclipse project or configuration is opened. */
    record Context(Version version) implements IExportContext {
        /** No workspace is involved in inspecting the serializer's schema. */
        public IProject getExportProject() { return null; }
        /** Select an installed platform profile explicitly. */
        public Version getProjectVersion() { return version; }
        /** Include properties from every compatibility mode. */
        public CompatibilityMode getCompatibilityMode() { return null; }
        /** XML schema version is independent of the platform profile. */
        public String getXmlFormatVersion() { return "2.20"; }
    }

    /** List exported properties and candidate nested fields for a subsequent manual review. */
    public static void main(String[] args) {
        var names = new MetadataFeatureNameProvider();
        var order = new MetadataObjectFeatureOrderProvider();
        var inspect = new Inspect();
        var output = new ArrayList<Object>();
        var classifiers = new ArrayList<EClassifier>(MdClassPackage.eINSTANCE.getEClassifiers());
        classifiers.addAll(com._1c.g5.v8.dt.mcore.McorePackage.eINSTANCE.getEClassifiers());
        for (var classifier : classifiers) {
            if (!(classifier instanceof EClass cls) || cls.isAbstract() || cls.isInterface()) continue;
            var properties = order.getProperties(cls, new Context(Version.V8_5_1));
            var fields = new ArrayList<Object>();
            for (var feature : cls.getEAllStructuralFeatures()) {
                if (!properties.contains(feature) && (feature.isTransient() || feature.isDerived())) continue;
                if (!inspect.isFeatureSupportedByVersion(feature, new Context(Version.V8_5_1))) continue;
                var row = new TreeMap<String, Object>();
                var qname = names.getElementQName(feature);
                row.put("namespace", qname.getNamespaceURI());
                row.put("xml", qname.getLocalPart());
                row.put("feature", feature.getName());
                row.put("owner", feature.getEContainingClass().getName());
                row.put("type", feature.getEType().getName());
                row.put("property", properties.contains(feature));
                row.put("since", inspect.isFeatureSupportedByVersion(feature, new Context(Version.V8_3_27))
                    ? "8.3.27" : "8.5.1");
                fields.add(row);
            }
            output.add(Map.of("class", cls.getName(), "parents",
                cls.getEAllSuperTypes().stream().map(EClass::getName).toList(), "fields", fields));
        }
        System.out.println(new GsonBuilder().setPrettyPrinting().create().toJson(output));
    }
}
