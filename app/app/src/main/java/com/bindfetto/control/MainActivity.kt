package com.bindfetto.control

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch

/** UI state for the control screen. */
data class ControlState(
    val host: String = "127.0.0.1",
    val port: String = "3491",
    val interfaces: List<String> = emptyList(),
    val selected: Set<String> = emptySet(),
    val status: String = "Not connected",
    val busy: Boolean = false,
)

/**
 * Holds the connection settings and the discovered/selected interface state, and drives
 * the [ControlClient]. The selectable list is seeded from `LIST` (observed interfaces)
 * with the boxes pre-checked from `GET` (the currently-active filter).
 */
class ControlViewModel : ViewModel() {
    private val _state = mutableStateOf(ControlState())
    val state: State<ControlState> get() = _state

    private fun update(block: (ControlState) -> ControlState) {
        _state.value = block(_state.value)
    }

    fun setHost(host: String) = update { it.copy(host = host) }
    fun setPort(port: String) = update { it.copy(port = port.filter(Char::isDigit)) }

    fun toggle(iface: String) = update {
        val next = it.selected.toMutableSet()
        if (!next.add(iface)) next.remove(iface)
        it.copy(selected = next)
    }

    private fun client(): ControlClient? {
        val port = _state.value.port.toIntOrNull() ?: return null
        return ControlClient(_state.value.host.trim(), port)
    }

    /** Pull the observed-interface list and the active filter, and merge them. */
    fun refresh() {
        val client = client() ?: run { update { it.copy(status = "Bad port") }; return }
        update { it.copy(busy = true, status = "Loading…") }
        viewModelScope.launch {
            try {
                val observed = client.list()
                val active = client.activeFilter().toSet()
                // Show observed plus any active-filter entries not (yet) observed, sorted.
                val all = (observed + active).distinct().sorted()
                update {
                    it.copy(
                        interfaces = all,
                        selected = active,
                        busy = false,
                        status = "${observed.size} interfaces • ${active.size} in filter",
                    )
                }
            } catch (e: Exception) {
                update { it.copy(busy = false, status = "Error: ${e.message}") }
            }
        }
    }

    /** Push the current selection as the in-kernel filter. */
    fun apply() {
        val client = client() ?: run { update { it.copy(status = "Bad port") }; return }
        val sel = _state.value.selected.toList().sorted()
        update { it.copy(busy = true, status = "Applying…") }
        viewModelScope.launch {
            try {
                val reply = client.set(sel)
                update { it.copy(busy = false, status = "SET ${sel.size}: $reply") }
            } catch (e: Exception) {
                update { it.copy(busy = false, status = "Error: ${e.message}") }
            }
        }
    }

    /** Clear the in-kernel filter (capture everything). */
    fun clear() {
        val client = client() ?: return
        update { it.copy(busy = true, status = "Clearing…") }
        viewModelScope.launch {
            try {
                val reply = client.set(emptyList())
                update { it.copy(selected = emptySet(), busy = false, status = "CLEAR: $reply") }
            } catch (e: Exception) {
                update { it.copy(busy = false, status = "Error: ${e.message}") }
            }
        }
    }
}

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    ControlScreen()
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ControlScreen(vm: ControlViewModel = viewModel()) {
    val s by vm.state
    Scaffold(topBar = { TopAppBar(title = { Text("bindfetto filter") }) }) { pad ->
        Column(modifier = Modifier.fillMaxSize().padding(pad).padding(16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    value = s.host,
                    onValueChange = vm::setHost,
                    label = { Text("Host") },
                    modifier = Modifier.width(200.dp),
                    singleLine = true,
                )
                Spacer(Modifier.width(8.dp))
                OutlinedTextField(
                    value = s.port,
                    onValueChange = vm::setPort,
                    label = { Text("Port") },
                    modifier = Modifier.width(100.dp),
                    singleLine = true,
                )
            }
            Spacer(Modifier.width(8.dp))
            Row(
                modifier = Modifier.fillMaxWidth().padding(vertical = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Button(onClick = vm::refresh, enabled = !s.busy) { Text("Refresh") }
                Button(onClick = vm::apply, enabled = !s.busy) { Text("Apply filter") }
                OutlinedButton(onClick = vm::clear, enabled = !s.busy) { Text("Clear") }
            }
            Text(s.status, style = MaterialTheme.typography.bodyMedium)
            LazyColumn(modifier = Modifier.fillMaxSize().padding(top = 8.dp)) {
                items(s.interfaces) { iface ->
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(vertical = 2.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Checkbox(
                            checked = iface in s.selected,
                            onCheckedChange = { vm.toggle(iface) },
                        )
                        Text(iface, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                    }
                }
            }
        }
    }
}
