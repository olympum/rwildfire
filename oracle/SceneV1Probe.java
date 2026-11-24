import java.io.PrintWriter;
import java.nio.charset.StandardCharsets;
import java.util.*;
import org.jwildfire.base.Prefs;
import org.jwildfire.create.tina.base.*;
import org.jwildfire.create.tina.base.raster.*;
import org.jwildfire.create.tina.io.FlameReader;
import org.jwildfire.create.tina.palette.RGBColor;
import org.jwildfire.create.tina.random.MarsagliaRandomGenerator;
import org.jwildfire.create.tina.render.*;
import org.jwildfire.create.tina.variation.*;

public class SceneV1Probe {
  public static void main(String[] args) throws Exception {
    Flame f = new FlameReader(Prefs.getPrefs()).readFlames(args[0]).get(0);
    Layer layer = f.getFirstLayer();
    List<XForm> xf = layer.getXForms();
    int w = f.getWidth()*2, h = f.getHeight()*2;
    f.setPixelsPerUnitScale(2.0);
    f.setAntialiasAmount(0.0);
    MarsagliaRandomGenerator rng = new MarsagliaRandomGenerator();
    rng.randomize(741103597L);
    FlameTransformationContext ctx = new FlameTransformationContext(null, rng, 0, 0);
    layer.refreshModWeightTables(ctx);
    FlameRendererView view = new FlameRendererView(Stereo3dEye.UNSPECIFIED, f, rng, 0, w, h, w, h, ctx, null);
    PrintWriter out = new PrintWriter(args[1], StandardCharsets.UTF_8.name());
    out.printf(Locale.ROOT,"scene %d %d %s %s %s %s %s %s %s %s%n",f.getWidth(),f.getHeight(),f.getPixelsPerUnit(),f.getCamZoom(),f.getCentreX(),f.getCentreY(),f.getBrightness(),f.getGamma(),f.getGammaThreshold(),f.getCamRoll());
    for (XForm x : xf) {
      out.printf(Locale.ROOT,"xform %s %s %s %s %s %s %s %s %s%n",x.getWeight(),x.getColor(),x.getColorSymmetry(),x.getXYCoeff00(),x.getXYCoeff01(),x.getXYCoeff10(),x.getXYCoeff11(),x.getXYCoeff20(),x.getXYCoeff21());
      for (Variation v : x.getVariations()) {
        if (v.getPriority() != 0 || v.getFunc().getPriority() != 0) {
          throw new AssertionError("expected ordinary variation");
        }
        String name = v.getFunc().getName();
        out.printf(Locale.ROOT, "variation %s %s", name, v.getAmount());
        int paramCount = name.equals("pdj") ? 4 : name.equals("rectangles") ? 2 : 0;
        if (paramCount > 0) {
          Object[] params = v.getFunc().getParameterValues();
          if (params.length != paramCount) throw new AssertionError("unexpected parameter count for " + name);
          for (Object param : params) out.print(" " + param);
        }
        out.println();
      }
      out.println("end-xform");
    }
    for(int i=0;i<256;i++) { RGBColor c=layer.getPalette().getColor(i); out.printf(Locale.ROOT,"palette %d %d %d%n",c.getRed(),c.getGreen(),c.getBlue()); }
    XYZPoint p=new XYZPoint(), aff=new XYZPoint(), var=new XYZPoint();
    p.x=2.0*rng.random()-1.0; p.y=2.0*rng.random()-1.0; p.color=rng.random(); p.material=rng.random();
    XForm current=xf.get(0); current.transformPoint(ctx,aff,var,p,p);
    for (int i=0;i<=Constants.INITIAL_ITERATIONS;i++) {
      current=current.getNextAppliedXFormTable()[rng.random(Constants.NEXT_APPLIED_XFORM_TABLE_SIZE)];
      current.transformPoint(ctx,aff,var,p,p);
    }
    RasterFloatInt raster=new RasterFloatInt(); raster.allocRaster(f,w,h,1,1.0);
    XYZProjectedPoint prj=new XYZProjectedPoint(0);
    PlotSample s=new PlotSample(); PlotSample[] samples={s};
    Set<Integer> bins=new TreeSet<>();
    for (int i=0;i<256;i++) {
      current=current.getNextAppliedXFormTable()[rng.random(Constants.NEXT_APPLIED_XFORM_TABLE_SIZE)];
      current.transformPoint(ctx,aff,var,p,p);
      int px=-1,py=-1;
      if (view.project(p,prj)) {
        px=(int)(view.getBws()*prj.x+0.5); py=(int)(view.getBhs()*prj.y+0.5);
        if(px<0 || px>=w || py<0 || py>=h) {px=-1;py=-1;}
      }
      out.printf(Locale.ROOT,"point %d %s %s %s %s %d %d%n",xf.indexOf(current),p.x,p.y,p.z,p.color,px,py);
      if(px>=0) {
        s.screenX=px; s.screenY=py; s.r=p.redColor; s.g=p.greenColor; s.b=p.blueColor;
        raster.addSamples(samples,1); bins.add(py*w+px);
      }
    }
    RasterPoint rp=new RasterPoint(0);
    for(int k:bins) {
      raster.readRasterPoint(k%w,k/w,rp);
      out.printf(Locale.ROOT,"bin %d %d %s %s %s%n",k,rp.count,rp.red,rp.green,rp.blue);
    }
    out.close();
    if(out.checkError()) throw new AssertionError("trace write failed");
  }
}
