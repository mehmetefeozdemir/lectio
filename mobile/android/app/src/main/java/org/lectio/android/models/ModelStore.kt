package org.lectio.android.models

import android.content.Context
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.Data
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkInfo
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import androidx.work.workDataOf
import java.io.File
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withContext
import uniffi.lectio_ffi.ModelInfo
import uniffi.lectio_ffi.SpeechEngine
import uniffi.lectio_ffi.StudyEngine
import uniffi.lectio_ffi.verifyDownload

/**
 * Model files on Android: downloaded by WorkManager (it survives the app
 * going to the background and retries after a network drop), checked by the
 * core, loaded on a background thread and kept for the rest of the launch.
 */
class ModelStore(private val context: Context, dataDirectory: File) : ModelProvider {
    private val modelsDirectory = File(dataDirectory, "models")
    private val speechEngines = mutableMapOf<String, SpeechEngine>()
    private val studyEngines = mutableMapOf<String, StudyEngine>()

    fun folder(model: ModelInfo): File = File(modelsDirectory, model.dirName)

    /** Every file is there with the right size (its checksum was checked when it arrived). */
    override fun isDownloaded(model: ModelInfo): Boolean =
        model.files.all { File(folder(model), it.name).length() == it.bytes.toLong() }

    override suspend fun download(model: ModelInfo, progress: (Double) -> Unit) {
        val request = OneTimeWorkRequestBuilder<DownloadWorker>()
            .setInputData(DownloadWorker.input(folder(model), model))
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
            .build()
        val work = WorkManager.getInstance(context)
        work.enqueueUniqueWork("model-${model.dirName}", ExistingWorkPolicy.KEEP, request)
        val finished = work.getWorkInfoByIdFlow(request.id).first { info ->
            info?.progress?.getDouble(DownloadWorker.KEY_FRACTION, -1.0)?.takeIf { it >= 0 }?.let(progress)
            info == null || info.state.isFinished
        }
        if (finished?.state != WorkInfo.State.SUCCEEDED) {
            throw IOException(finished?.outputData?.getString(DownloadWorker.KEY_ERROR) ?: "The download of ${model.displayName} stopped.")
        }
        progress(1.0)
    }

    override suspend fun speechEngine(model: ModelInfo): SpeechEngine = speechEngines[model.name]
        ?: withContext(Dispatchers.Default) { SpeechEngine.load(folder(model).path, model.name) }
            .also { speechEngines[model.name] = it }

    override suspend fun studyEngine(model: ModelInfo): StudyEngine = studyEngines[model.name]
        ?: withContext(Dispatchers.Default) { StudyEngine.load(folder(model).path, model.name) }
            .also { studyEngines[model.name] = it }
}

/**
 * Downloads one model's files into `<file>.partial`, has the core check each
 * one's size and SHA-256, then moves it into place, so a half-finished file
 * is never mistaken for the model.
 */
class DownloadWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val folder = File(inputData.getString(KEY_FOLDER) ?: return failure("No folder for the model."))
        val names = strings(KEY_NAMES)
        val urls = strings(KEY_URLS)
        val sizes = inputData.getLongArray(KEY_BYTES) ?: LongArray(0)
        val sums = strings(KEY_SHA256)
        if (setOf(names.size, urls.size, sizes.size, sums.size).size != 1) return failure("The model's file list is incomplete.")
        folder.mkdirs()
        val total = sizes.sum().coerceAtLeast(1).toDouble()
        var done = 0L
        for (i in names.indices) {
            val target = File(folder, names[i])
            if (target.length() != sizes[i]) {
                val partial = File(folder, names[i] + ".partial")
                try {
                    fetch(urls[i], partial) { got -> setProgress(workDataOf(KEY_FRACTION to (done + got) / total)) }
                } catch (e: IOException) {
                    return if (runAttemptCount < MAX_RETRIES) Result.retry() else failure("The download of ${names[i]} failed: ${e.message}")
                }
                if (!verifyDownload(partial.path, sizes[i].toULong(), sums[i])) {
                    partial.delete()
                    return failure("The download of ${names[i]} was damaged (its checksum didn't match). Try again.")
                }
                target.delete()
                if (!partial.renameTo(target)) return failure("Couldn't save ${names[i]}.")
            }
            done += sizes[i]
            setProgress(workDataOf(KEY_FRACTION to done / total))
        }
        return Result.success()
    }

    private suspend fun fetch(url: String, destination: File, progress: suspend (Long) -> Unit) = withContext(Dispatchers.IO) {
        val connection = URL(url).openConnection() as HttpURLConnection
        try {
            connection.instanceFollowRedirects = true
            if (connection.responseCode != HttpURLConnection.HTTP_OK) throw IOException("the server answered ${connection.responseCode}")
            connection.inputStream.use { input ->
                destination.outputStream().use { output ->
                    val buffer = ByteArray(BUFFER_BYTES)
                    var got = 0L
                    var reported = 0L
                    while (true) {
                        val n = input.read(buffer)
                        if (n < 0) break
                        output.write(buffer, 0, n)
                        got += n
                        if (got - reported >= REPORT_EVERY_BYTES) {
                            reported = got
                            progress(got)
                        }
                    }
                }
            }
        } finally {
            connection.disconnect()
        }
    }

    private fun failure(message: String) = Result.failure(workDataOf(KEY_ERROR to message))

    private fun strings(key: String): List<String> = inputData.getNullableStringArray(key)?.map { it.orEmpty() }.orEmpty()

    companion object {
        const val KEY_FOLDER = "folder"
        const val KEY_NAMES = "names"
        const val KEY_URLS = "urls"
        const val KEY_BYTES = "bytes"
        const val KEY_SHA256 = "sha256"
        const val KEY_FRACTION = "fraction"
        const val KEY_ERROR = "error"
        private const val MAX_RETRIES = 3
        private const val BUFFER_BYTES = 1 shl 16
        private const val REPORT_EVERY_BYTES = 1L shl 20

        fun input(folder: File, model: ModelInfo): Data = workDataOf(
            KEY_FOLDER to folder.path,
            KEY_NAMES to model.files.map { it.name }.toTypedArray(),
            KEY_URLS to model.files.map { it.url }.toTypedArray(),
            KEY_BYTES to model.files.map { it.bytes.toLong() }.toLongArray(),
            KEY_SHA256 to model.files.map { it.sha256 }.toTypedArray(),
        )
    }
}
