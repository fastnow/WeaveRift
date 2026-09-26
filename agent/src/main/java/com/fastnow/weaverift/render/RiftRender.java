package com.fastnow.weaverift.render;

import org.lwjgl.opengl.GL11;

public class RiftRender {

    private static long frameCount = 0;

    public static void onRender() {
        frameCount++;
        if (frameCount % 60 != 0) return;

        try {
            drawTestBox();
        } catch (Throwable t) {
            t.printStackTrace();
        }
    }

    private static void drawTestBox() {
        GL11.glPushMatrix();
        GL11.glPushAttrib(GL11.GL_ALL_ATTRIB_BITS);

        GL11.glDisable(GL11.GL_TEXTURE_2D);
        GL11.glDisable(GL11.GL_DEPTH_TEST);
        GL11.glEnable(GL11.GL_BLEND);
        GL11.glBlendFunc(GL11.GL_SRC_ALPHA, GL11.GL_ONE_MINUS_SRC_ALPHA);

        GL11.glColor4f(1.0f, 0.0f, 0.0f, 0.5f);

        GL11.glBegin(GL11.GL_QUADS);
        GL11.glVertex2f(100, 100);
        GL11.glVertex2f(200, 100);
        GL11.glVertex2f(200, 200);
        GL11.glVertex2f(100, 200);
        GL11.glEnd();

        GL11.glPopAttrib();
        GL11.glPopMatrix();

        System.out.println("[WeaveRift] Drew test box (frame " + frameCount + ")");
    }
}