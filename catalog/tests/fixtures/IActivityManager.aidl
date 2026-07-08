/*
 * A trimmed, illustrative interface (not the real AOSP one).
 */
package android.app;

import android.content.Intent;

/** Activity manager. */
interface IActivityManager {
    List<RunningTaskInfo> getTasks(int maxNum);

    // Starting an activity — spans multiple lines on purpose.
    int startActivity(in Intent intent,
                      String resolvedType);

    oneway void noteWakeupAlarm(in PendingIntent ps);
}
